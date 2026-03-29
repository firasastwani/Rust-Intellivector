use crate::context_group::{build_groups, rank_groups};
use crate::embed::Embedder;
use crate::ingest::{map_file, split_chunk_spans, split_code_chunks, wrap_chunk_spans};
use crate::search::bm25::Bm25Index;
use crate::search::rrf::rrf_fuse;
use crate::search::vector_index::VectorIndex;
use crate::storage::hash::hash_chunk;
use crate::storage::types::{ChunkId, ChunkMeta, FileFingerprint};
use crate::storage::{PersistentStore, sled_store::SledStore};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::io;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub struct Entry {
    pub id: ChunkId,
    pub meta: ChunkMeta,
    pub embedding: Vec<f32>,
}

pub struct VectorStore {
    entries: Vec<Entry>,
    project_root: PathBuf,
    file_hashes: HashMap<PathBuf, u64>,
    vector_index: VectorIndex,
}

impl VectorStore {
    pub fn new(project_root: PathBuf, dimension: usize) -> Self {
        VectorStore {
            entries: Vec::new(),
            project_root,
            file_hashes: HashMap::new(),
            vector_index: VectorIndex::new(dimension),
        }
    }

    pub fn load_from_store(
        db: &SledStore,
        project_root: PathBuf,
        dimension: usize,
    ) -> anyhow::Result<Self> {
        let mut store = VectorStore::new(project_root, dimension);
        let file_hashes = db.iter_file_hashes()?;
        for (rel_path, hash) in file_hashes {
            store.file_hashes.insert(rel_path.clone(), hash);
            if let Some(ids) = db.get_file_index(&rel_path)? {
                for id in ids {
                    let (Some(meta), Some(embedding)) = (db.get_meta(&id)?, db.get_embedding(&id)?) else {
                        continue;
                    };
                    store.entries.push(Entry {
                        id,
                        meta,
                        embedding: embedding.clone(),
                    });
                }
            }
        }
        let items: Vec<(ChunkId, Vec<f32>)> = store
            .entries
            .iter()
            .map(|e| (e.id, e.embedding.clone()))
            .collect();
        store.vector_index.rebuild(dimension, &items);
        Ok(store)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn index_project(
        &mut self,
        root: &Path,
        embedder: &Embedder,
        db: &SledStore,
        bm25: &mut Bm25Index,
    ) -> anyhow::Result<()> {
        if !root.exists() || !root.is_dir() {
            anyhow::bail!("project root is not a directory: {:?}", root);
        }
        self.project_root = root.to_path_buf();
        db.set_project_root(root)?;

        let files = walk_files(root)?;
        if files.is_empty() {
            println!("[warn] no files found under {:?}", root);
            return Ok(());
        }
        let mut pb = None;
        if std::io::stdout().is_terminal() {
            let bar = ProgressBar::new(files.len() as u64);
            bar.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {bar:40} {pos}/{len} {msg}")?,
            );
            pb = Some(bar);
        } else {
            println!("Indexing {} files...", files.len());
        }

        for (idx, file) in files.iter().enumerate() {
            if let Some(bar) = &pb {
                bar.set_message(file.display().to_string());
            } else {
                println!("[{}/{}] {}", idx + 1, files.len(), file.display());
            }
            if let Err(err) = self.index_file(root, &file, embedder, db, bm25) {
                eprintln!("[warn] failed to index {:?}: {}", file, err);
            }
            if let Some(bar) = &pb {
                bar.inc(1);
            }
        }
        if let Some(bar) = pb {
            bar.finish_with_message("Indexing complete");
        } else {
            println!("Indexing complete");
        }

        self.rebuild_vector_index();
        Ok(())
    }

    pub fn update_project(
        &mut self,
        root: &Path,
        embedder: &Embedder,
        db: &SledStore,
        bm25: &mut Bm25Index,
    ) -> anyhow::Result<()> {
        let files = walk_files(root)?;
        if files.is_empty() {
            println!("[warn] no files found under {:?}", root);
            return Ok(());
        }
        let mut on_disk: HashSet<PathBuf> = HashSet::new();
        let mut changed = false;

        let mut pb = None;
        if std::io::stdout().is_terminal() {
            let bar = ProgressBar::new(files.len() as u64);
            bar.set_style(
                ProgressStyle::default_bar()
                    .template("[{elapsed_precise}] {bar:40} {pos}/{len} {msg}")?,
            );
            pb = Some(bar);
        } else {
            println!("Scanning {} files for updates...", files.len());
        }

        for (idx, file) in files.iter().enumerate() {
            if let Some(bar) = &pb {
                bar.set_message(file.display().to_string());
            } else {
                println!("[{}/{}] {}", idx + 1, files.len(), file.display());
            }
            let rel_path = relative_path(root, &file)?;
            on_disk.insert(rel_path.clone());
            let hash = hash_file(&file)?;
            let prev = self.file_hashes.get(&rel_path).copied();
            if prev.map(|p| p != hash).unwrap_or(true) {
                if let Err(err) = self.index_file(root, &file, embedder, db, bm25) {
                    eprintln!("[warn] failed to update {:?}: {}", file, err);
                } else {
                    changed = true;
                }
            }
            if let Some(bar) = &pb {
                bar.inc(1);
            }
        }
        if let Some(bar) = pb {
            bar.finish_with_message("Update scan complete");
        } else {
            println!("Update scan complete");
        }

        let existing: Vec<PathBuf> = self.file_hashes.keys().cloned().collect();
        for rel_path in existing {
            if !on_disk.contains(&rel_path) {
                self.remove_chunks_for_file(&rel_path, db, bm25)?;
                self.file_hashes.remove(&rel_path);
                db.remove_file_hash(&rel_path)?;
                changed = true;
            }
        }

        if changed {
            self.rebuild_vector_index();
        }
        Ok(())
    }

    pub fn index_file(
        &mut self,
        root: &Path,
        path: &Path,
        embedder: &Embedder,
        db: &SledStore,
        bm25: &mut Bm25Index,
    ) -> anyhow::Result<()> {
        let rel_path = relative_path(root, path)?;
        let hash = hash_file(path)?;

        self.remove_chunks_for_file(&rel_path, db, bm25)?;

        let fp = file_fingerprint(path)?;
        let file = std::fs::File::open(path)?;
        let mmap = map_file(file);

        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        let chunks = if ext == "rs" {
            match split_code_chunks(&rel_path, &mmap, fp.modified) {
                Ok(chunks) => chunks,
                Err(err) => {
                    return Err(anyhow::anyhow!("failed to parse {:?}: {}", path, err));
                }
            }
        } else {
            let spans = split_chunk_spans(&mmap, 512);
            wrap_chunk_spans(&rel_path, spans, fp.modified)
        };

        let mut ids = Vec::with_capacity(chunks.len());

        for code_chunk in chunks.iter() {
            let (start, end) = code_chunk.span;
            let chunk_bytes = &mmap[start..end];
            let id = hash_chunk(chunk_bytes);
            ids.push(id);

            let mut meta = code_chunk.meta.clone();
            meta.file_path = rel_path.clone();
            meta.file_hash = hash;

            let embedding = if let Some(emb) = db.get_embedding(&id)? {
                emb
            } else {
                let text = std::str::from_utf8(chunk_bytes)?;
                let emb = embedder.embed(text)?;
                db.put_embedding(&id, &emb)?;
                emb
            };

            db.put_meta(&id, &meta)?;
            self.entries.push(Entry {
                id,
                meta: meta.clone(),
                embedding: embedding.clone(),
            });
            self.vector_index.insert(id, &embedding);

            let text = std::str::from_utf8(chunk_bytes)?;
            let search_text = build_search_text(&meta, text);
            bm25.index_chunk(id, &meta, &search_text)?;
        }

        db.set_file_index(&rel_path, &ids)?;
        db.set_file_fingerprint(&rel_path, &fp)?;
        db.set_file_hash(&rel_path, hash)?;
        self.file_hashes.insert(rel_path.clone(), hash);
        bm25.commit()?;
        Ok(())
    }

    pub fn remove_chunks_for_file(
        &mut self,
        rel_path: &Path,
        db: &SledStore,
        bm25: &mut Bm25Index,
    ) -> anyhow::Result<()> {
        if let Some(ids) = db.get_file_index(rel_path)? {
            for id in ids {
                db.remove_meta(&id)?;
                db.remove_embedding(&id)?;
            }
        }
        db.remove_file_index(rel_path)?;
        db.remove_file_hash(rel_path)?;
        bm25.remove_file(rel_path)?;
        self.entries.retain(|e| e.meta.file_path != rel_path);
        Ok(())
    }

    pub fn search_groups(
        &self,
        query: &str,
        embedder: &Embedder,
        bm25: &Bm25Index,
        top_k: usize,
    ) -> anyhow::Result<Vec<crate::context_group::GroupResult>> {
        let mut metas: HashMap<ChunkId, ChunkMeta> = HashMap::new();
        let mut chunk_list = Vec::new();
        for entry in &self.entries {
            metas.insert(entry.id, entry.meta.clone());
            chunk_list.push((entry.id, entry.meta.clone()));
        }
        let group_index = build_groups(&chunk_list);

        let query_embedding = embedder.embed(query)?;
        let bm25_ids = bm25.search(query, 120)?;
        let vector_ids: Vec<ChunkId> = self
            .vector_index
            .search(&query_embedding, 120)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let rrf_scores = rrf_fuse(&[bm25_ids, vector_ids], 60.0);
        Ok(rank_groups(&group_index, &metas, &rrf_scores, top_k))
    }

    pub fn stats(&self) -> (usize, usize) {
        (self.file_hashes.len(), self.entries.len())
    }

    fn rebuild_vector_index(&mut self) {
        let items: Vec<(ChunkId, Vec<f32>)> = self
            .entries
            .iter()
            .map(|e| (e.id, e.embedding.clone()))
            .collect();
        let dimension = items.get(0).map(|(_, e)| e.len()).unwrap_or(384);
        self.vector_index.rebuild(dimension, &items);
    }
}

pub fn hash_file(path: &Path) -> anyhow::Result<u64> {
    let content = std::fs::read(path)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    Ok(hasher.finish())
}

fn walk_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_ignored_dir(e.path()))
        .filter_map(Result::ok)
    {
        if entry.file_type().is_file() {
                files.push(entry.path().to_path_buf());
        }
    }
    Ok(files)
}

fn is_ignored_dir(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
        return matches!(name, "target" | ".git" | "vector_db");
    }
    false
}

fn relative_path(root: &Path, path: &Path) -> anyhow::Result<PathBuf> {
    Ok(path.strip_prefix(root)?.to_path_buf())
}

fn file_fingerprint(path: &Path) -> anyhow::Result<FileFingerprint> {
    let meta = std::fs::metadata(path)?;
    let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let modified = modified
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Ok(FileFingerprint {
        size: meta.len(),
        modified,
    })
}

fn build_search_text(meta: &ChunkMeta, text: &str) -> String {
    let mut parts = Vec::new();
    if let Some(symbol) = meta.symbol_name.as_deref() {
        parts.push(symbol);
    }
    if let Some(signature) = meta.signature.as_deref() {
        parts.push(signature);
    }
    if let Some(module) = meta.module_path.as_deref() {
        parts.push(module);
    }
    let path = meta.file_path.to_string_lossy();
    parts.push(&path);
    parts.push(text);
    parts.join("\n")
}

pub fn load_chunk_text(project_root: &Path, meta: &ChunkMeta) -> anyhow::Result<String> {
    let full_path = project_root.join(&meta.file_path);
    let bytes = std::fs::read(&full_path)?;
    let start = meta.byte_start as usize;
    let end = meta.byte_end as usize;
    if start >= bytes.len() || end > bytes.len() || start >= end {
        return Ok(String::from("[invalid chunk range]"));
    }
    Ok(String::from_utf8_lossy(&bytes[start..end]).to_string())
}

pub fn project_hash(root: &Path) -> anyhow::Result<u64> {
    let canonical = root.canonicalize()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    canonical.to_string_lossy().hash(&mut hasher);
    Ok(hasher.finish())
}

pub fn active_project_file() -> PathBuf {
    PathBuf::from("vector_db/active_project")
}

pub fn write_active_project(root: &Path, hash: u64) -> anyhow::Result<()> {
    let content = format!("{}\n{}", hash, root.display());
    std::fs::create_dir_all("vector_db")?;
    std::fs::write(active_project_file(), content)?;
    Ok(())
}

pub fn read_active_project() -> anyhow::Result<(u64, PathBuf)> {
    let content = std::fs::read_to_string(active_project_file())?;
    let mut lines = content.lines();
    let hash = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing hash"))?
        .parse::<u64>()?;
    let root = lines
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing root"))?;
    Ok((hash, PathBuf::from(root)))
}
