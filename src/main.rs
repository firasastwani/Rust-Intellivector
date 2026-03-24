use crate::context_group::{build_groups, member_ids, rank_groups};
use crate::embed::Embedder;
use crate::ingest::{map_file, split_chunk_spans, split_code_chunks, wrap_chunk_spans};
use crate::search::bm25::Bm25Index;
use crate::search::rrf::rrf_fuse;
use crate::storage::PersistentStore;
use crate::storage::hash::hash_chunk;
use crate::storage::sled_store::SledStore;
use crate::storage::types::{ChunkMeta, FileFingerprint};
use crate::store::VectorStore;
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

mod code_chunker;
mod context_group;
mod embed;
mod ingest;
mod search;
mod similarity;
mod storage;
mod store;

#[derive(Parser)]
#[command(name = "VectorTool")]
#[command(
    about = "Reads either a .txt or .md file and stores its Vector embeddings then answers queries"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Ingest { path: PathBuf },
    Eval { path: PathBuf, queries: PathBuf },
}

fn main() -> anyhow::Result<()> {
    let embedder = Embedder::load()?;
    let mut store = VectorStore::new();
    let db = SledStore::open("vector_db")?;
    let mut bm25 = Bm25Index::open(Path::new("vector_db/bm25"))?;

    let cli = Cli::parse();

    match &cli.command {
        Commands::Ingest { path } => {
            ingest_file(path, &embedder, &mut store, &db, &mut bm25)?;
            query_loop(&embedder, &store, &bm25)?;
        }
        Commands::Eval { path, queries } => {
            ingest_file(path, &embedder, &mut store, &db, &mut bm25)?;
            run_eval(&embedder, &store, &bm25, queries)?;
        }
    }

    Ok(())
}

fn load_file_index_into_store(
    db: &SledStore,
    store: &mut VectorStore,
    path: &PathBuf,
) -> anyhow::Result<()> {
    let Some(ids) = db.get_file_index(path)? else {
        return Ok(());
    };

    for id in ids {
        let (Some(meta), Some(embedding)) = (db.get_meta(&id)?, db.get_embedding(&id)?) else {
            continue;
        };
        store.insert(id, meta, embedding);
    }

    Ok(())
}

fn ingest_file(
    path: &PathBuf,
    embedder: &Embedder,
    store: &mut VectorStore,
    db: &SledStore,
    bm25: &mut Bm25Index,
) -> anyhow::Result<()> {
    let fp = file_fingerprint(path)?;
    if let Some(prev) = db.get_file_fingerprint(path)? {
        if prev == fp {
            println!("No changes detected for {:?}", path);
            load_file_index_into_store(db, store, path)?;
            return Ok(());
        }
        db.remove_file_index(path)?;
        bm25.remove_file(path)?;
    }

    let file = File::open(path)?;
    let mmap = map_file(file);

    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    let chunks = if ext == "rs" {
        split_code_chunks(path, &mmap, fp.modified)
    } else {
        let spans = split_chunk_spans(&mmap, 512);
        wrap_chunk_spans(path, spans, fp.modified)
    };

    let mut ids = Vec::with_capacity(chunks.len());

    for code_chunk in chunks.iter() {
        let (start, end) = code_chunk.span;
        let chunk_bytes = &mmap[start..end];
        let id = hash_chunk(chunk_bytes);
        ids.push(id);

        let meta = code_chunk.meta.clone();

        let embedding = if let Some(emb) = db.get_embedding(&id)? {
            emb
        } else {
            let text = std::str::from_utf8(chunk_bytes)?;
            let emb = embedder.embed(text)?;
            db.put_embedding(&id, &emb)?;
            emb
        };

        db.put_meta(&id, &meta)?;
        store.insert(id, meta, embedding);

        let text = std::str::from_utf8(chunk_bytes)?;
        let search_text = build_search_text(&code_chunk.meta, text);
        bm25.index_chunk(id, &code_chunk.meta, &search_text)?;
    }

    db.set_file_index(path, &ids)?;
    db.set_file_fingerprint(path, &fp)?;
    bm25.commit()?;

    Ok(())
}

fn query_loop(embedder: &Embedder, store: &VectorStore, bm25: &Bm25Index) -> anyhow::Result<()> {
    println!(
        "Ready ({} chunks). Type a question (or 'exit' to quit):",
        store.len()
    );

    let mut metas: HashMap<_, _> = HashMap::new();
    let mut chunk_list = Vec::new();
    for entry in store.entries() {
        metas.insert(entry.id, entry.meta.clone());
        chunk_list.push((entry.id, entry.meta.clone()));
    }
    let group_index = build_groups(&chunk_list);

    let stdin = io::stdin();
    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        let question = line.trim();

        if question == "exit" || question.is_empty() {
            break;
        }

        let results = run_group_search(question, embedder, store, bm25, &group_index, &metas)?;

        println!(
            "\nTop results for: \"{}\"\n",
            &question[..50.min(question.len())]
        );
        for (i, result) in results.iter().enumerate() {
            let primary = crate::context_group::primary_id(&result.group);
            let primary_meta = metas.get(&primary);
            println!("--- Result {} ---", i + 1);
            println!("Score: {:.4}", result.score);
            if let Some(meta) = primary_meta {
                let symbol = meta.symbol_name.as_deref().unwrap_or("[unknown]");
                let symbol_type = meta
                    .symbol_type
                    .as_ref()
                    .map(|s| format!("{:?}", s))
                    .unwrap_or_else(|| "Unknown".to_string());
                println!(
                    "Symbol: {} ({})\nFile: {}",
                    symbol,
                    symbol_type,
                    meta.file_path.to_string_lossy()
                );
            }

            let mut members = member_ids(&result.group);
            members.sort_by(|a, b| {
                let ma = metas.get(a);
                let mb = metas.get(b);
                match (ma, mb) {
                    (Some(ma), Some(mb)) => {
                        let pa = ma.file_path.to_string_lossy();
                        let pb = mb.file_path.to_string_lossy();
                        let file_cmp = pa.cmp(&pb);
                        if file_cmp == std::cmp::Ordering::Equal {
                            ma.byte_start.cmp(&mb.byte_start)
                        } else {
                            file_cmp
                        }
                    }
                    _ => std::cmp::Ordering::Equal,
                }
            });
            members.dedup();

            for id in members {
                if let Some(meta) = metas.get(&id) {
                    let text = load_chunk_text(meta)?;
                    println!("{}\n", text);
                }
            }
        }
    }

    Ok(())
}

fn run_group_search(
    question: &str,
    embedder: &Embedder,
    store: &VectorStore,
    bm25: &Bm25Index,
    group_index: &crate::context_group::GroupIndex,
    metas: &HashMap<crate::storage::types::ChunkId, ChunkMeta>,
) -> anyhow::Result<Vec<crate::context_group::GroupResult>> {
    let query_embedding = embedder.embed(question)?;
    let bm25_ids = bm25.search(question, 120)?;
    let vector_ids = store.search_ids(&query_embedding, 120);
    let rrf_scores = rrf_fuse(&[bm25_ids, vector_ids], 60.0);
    Ok(rank_groups(group_index, metas, &rrf_scores, 5))
}

#[derive(Debug, Deserialize)]
struct EvalQuery {
    query: String,
    expected_symbol: String,
    expected_file: Option<String>,
    rationale: Option<String>,
}

fn run_eval(
    embedder: &Embedder,
    store: &VectorStore,
    bm25: &Bm25Index,
    queries_path: &PathBuf,
) -> anyhow::Result<()> {
    let mut metas: HashMap<_, _> = HashMap::new();
    let mut chunk_list = Vec::new();
    for entry in store.entries() {
        metas.insert(entry.id, entry.meta.clone());
        chunk_list.push((entry.id, entry.meta.clone()));
    }
    let group_index = build_groups(&chunk_list);

    let content = std::fs::read_to_string(queries_path)?;
    let mut total = 0u64;
    let mut hit = 0u64;
    let mut mrr_sum = 0.0f64;
    let mut precision_sum = 0.0f64;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let q: EvalQuery = serde_json::from_str(line)?;
        total += 1;
        let results = run_group_search(&q.query, embedder, store, bm25, &group_index, &metas)?;

        let mut found_rank: Option<usize> = None;
        let mut relevant_count = 0usize;
        for (idx, result) in results.iter().enumerate() {
            let primary = crate::context_group::primary_id(&result.group);
            let meta = metas.get(&primary);
            let mut symbol_match = false;
            let mut file_match = false;
            if let Some(m) = meta {
                if let Some(symbol) = m.symbol_name.as_deref() {
                    symbol_match = symbol == q.expected_symbol;
                }
                if let Some(expected_file) = q.expected_file.as_deref() {
                    let path = m.file_path.to_string_lossy();
                    file_match = path.contains(expected_file);
                }
            }
            let relevant = symbol_match || file_match;
            if relevant {
                relevant_count += 1;
                if found_rank.is_none() {
                    found_rank = Some(idx + 1);
                }
            }
        }

        if found_rank.is_some() {
            hit += 1;
            mrr_sum += 1.0 / found_rank.unwrap() as f64;
        }
        precision_sum += relevant_count as f64 / 5.0;
    }

    if total == 0 {
        println!("No evaluation queries found in {:?}", queries_path);
        return Ok(());
    }

    let recall = hit as f64 / total as f64;
    let mrr = mrr_sum / total as f64;
    let precision = precision_sum / total as f64;
    println!("Eval results over {} queries:", total);
    println!("Symbol recall: {:.3}", recall);
    println!("MRR: {:.3}", mrr);
    println!("Precision@5: {:.3}", precision);
    Ok(())
}

fn file_fingerprint(path: &PathBuf) -> anyhow::Result<FileFingerprint> {
    let meta = std::fs::metadata(path)?;
    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let modified = modified
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Ok(FileFingerprint {
        size: meta.len(),
        modified,
    })
}

fn load_chunk_text(meta: &ChunkMeta) -> anyhow::Result<String> {
    let bytes = std::fs::read(&meta.file_path)?;
    let start = meta.byte_start as usize;
    let end = meta.byte_end as usize;
    if start >= bytes.len() || end > bytes.len() || start >= end {
        return Ok(String::from("[invalid chunk range]"));
    }
    Ok(String::from_utf8_lossy(&bytes[start..end]).to_string())
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
