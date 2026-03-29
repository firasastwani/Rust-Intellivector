pub mod hash;
pub mod sled_store;
pub mod types;

use crate::storage::types::{ChunkId, ChunkMeta, FileFingerprint};
use std::path::{Path, PathBuf};

pub trait PersistentStore {
    fn get_embedding(&self, id: &ChunkId) -> anyhow::Result<Option<Vec<f32>>>;
    fn put_embedding(&self, id: &ChunkId, embedding: &[f32]) -> anyhow::Result<()>;
    fn remove_embedding(&self, id: &ChunkId) -> anyhow::Result<()>;

    fn get_meta(&self, id: &ChunkId) -> anyhow::Result<Option<ChunkMeta>>;
    fn put_meta(&self, id: &ChunkId, meta: &ChunkMeta) -> anyhow::Result<()>;
    fn remove_meta(&self, id: &ChunkId) -> anyhow::Result<()>;

    fn get_file_index(&self, path: &Path) -> anyhow::Result<Option<Vec<ChunkId>>>;
    fn set_file_index(&self, path: &Path, ids: &[ChunkId]) -> anyhow::Result<()>;
    fn remove_file_index(&self, path: &Path) -> anyhow::Result<()>;

    fn set_file_fingerprint(&self, path: &Path, fp: &FileFingerprint) -> anyhow::Result<()>;

    fn set_file_hash(&self, path: &Path, hash: u64) -> anyhow::Result<()>;
    fn remove_file_hash(&self, path: &Path) -> anyhow::Result<()>;
    fn iter_file_hashes(&self) -> anyhow::Result<Vec<(PathBuf, u64)>>;

    fn set_project_root(&self, path: &Path) -> anyhow::Result<()>;
}
