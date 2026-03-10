use crate::storage::types::{ChunkId, ChunkMeta, FileFingerprint};
use crate::storage::PersistentStore;
use sled::{Db, Tree};
use std::path::Path;

pub struct SledStore {
    _db: Db,
    embeddings: Tree,
    meta: Tree,
    file_index: Tree,
    file_fingerprint: Tree,
}

impl SledStore {
    pub fn open<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let db = sled::open(path)?;
        let embeddings = db.open_tree("embeddings")?;
        let meta = db.open_tree("meta")?;
        let file_index = db.open_tree("file_index")?;
        let file_fingerprint = db.open_tree("file_fingerprint")?;

        Ok(SledStore {
            _db: db,
            embeddings,
            meta,
            file_index,
            file_fingerprint,
        })
    }

    fn key_for_id(id: &ChunkId) -> [u8; 32] {
        id.hash
    }

    fn key_for_path(path: &Path) -> Vec<u8> {
        path.to_string_lossy().as_bytes().to_vec()
    }
}

impl PersistentStore for SledStore {
    fn get_embedding(&self, id: &ChunkId) -> anyhow::Result<Option<Vec<f32>>> {
        let key = Self::key_for_id(id);
        let Some(value) = self.embeddings.get(key)? else {
            return Ok(None);
        };
        let embedding: Vec<f32> = bincode::deserialize(&value)?;
        Ok(Some(embedding))
    }

    fn put_embedding(&self, id: &ChunkId, embedding: &[f32]) -> anyhow::Result<()> {
        let key = Self::key_for_id(id);
        let bytes = bincode::serialize(embedding)?;
        self.embeddings.insert(key, bytes)?;
        Ok(())
    }

    fn get_meta(&self, id: &ChunkId) -> anyhow::Result<Option<ChunkMeta>> {
        let key = Self::key_for_id(id);
        let Some(value) = self.meta.get(key)? else {
            return Ok(None);
        };
        let meta: ChunkMeta = bincode::deserialize(&value)?;
        Ok(Some(meta))
    }

    fn put_meta(&self, id: &ChunkId, meta: &ChunkMeta) -> anyhow::Result<()> {
        let key = Self::key_for_id(id);
        let bytes = bincode::serialize(meta)?;
        self.meta.insert(key, bytes)?;
        Ok(())
    }

    fn get_file_index(&self, path: &Path) -> anyhow::Result<Option<Vec<ChunkId>>> {
        let key = Self::key_for_path(path);
        let Some(value) = self.file_index.get(key)? else {
            return Ok(None);
        };
        let ids: Vec<ChunkId> = bincode::deserialize(&value)?;
        Ok(Some(ids))
    }

    fn set_file_index(&self, path: &Path, ids: &[ChunkId]) -> anyhow::Result<()> {
        let key = Self::key_for_path(path);
        let bytes = bincode::serialize(ids)?;
        self.file_index.insert(key, bytes)?;
        Ok(())
    }

    fn remove_file_index(&self, path: &Path) -> anyhow::Result<()> {
        let key = Self::key_for_path(path);
        self.file_index.remove(key)?;
        Ok(())
    }

    fn get_file_fingerprint(&self, path: &Path) -> anyhow::Result<Option<FileFingerprint>> {
        let key = Self::key_for_path(path);
        let Some(value) = self.file_fingerprint.get(key)? else {
            return Ok(None);
        };
        let fp: FileFingerprint = bincode::deserialize(&value)?;
        Ok(Some(fp))
    }

    fn set_file_fingerprint(&self, path: &Path, fp: &FileFingerprint) -> anyhow::Result<()> {
        let key = Self::key_for_path(path);
        let bytes = bincode::serialize(fp)?;
        self.file_fingerprint.insert(key, bytes)?;
        Ok(())
    }
}
