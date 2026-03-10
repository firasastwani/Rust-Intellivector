use crate::similarity::cosine_similarity;
use crate::storage::types::{ChunkId, ChunkMeta};

pub struct Entry {
    pub id: ChunkId,
    pub meta: ChunkMeta,
    pub embedding: Vec<f32>,
}

pub struct VectorStore {
    entries: Vec<Entry>,
}

impl VectorStore {
    pub fn new() -> Self {
        VectorStore {
            entries: Vec::new(),
        }
    }

    pub fn insert(&mut self, id: ChunkId, meta: ChunkMeta, embedding: Vec<f32>) {
        self.entries.push(Entry { id, meta, embedding });
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn search(&self, query_embedding: &[f32], top_k: usize) -> Vec<(&Entry, f32)> {
        let mut scored: Vec<(&Entry, f32)> = self
            .entries
            .iter()
            .map(|entry| (entry, cosine_similarity(query_embedding, &entry.embedding)))
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Less));

        scored.into_iter().take(top_k).collect()
    }
}
