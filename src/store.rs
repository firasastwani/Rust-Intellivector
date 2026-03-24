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

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn get_entry(&self, id: &ChunkId) -> Option<&Entry> {
        self.entries.iter().find(|e| &e.id == id)
    }

    // take similirty algo as a future param and swap out
    pub fn search(&self, query_embedding: &[f32], top_k: usize) -> Vec<(&Entry, f32)> {
        let mut scored: Vec<(&Entry, f32)> = self
            .entries
            .iter()
            .map(|entry| (entry, cosine_similarity(query_embedding, &entry.embedding)))
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Less));

        scored.into_iter().take(top_k).collect()
    }

    pub fn search_ids(&self, query_embedding: &[f32], top_k: usize) -> Vec<ChunkId> {
        let mut scored: Vec<(&Entry, f32)> = self
            .entries
            .iter()
            .map(|entry| (entry, cosine_similarity(query_embedding, &entry.embedding)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Less));
        scored
            .into_iter()
            .take(top_k)
            .map(|(e, _)| e.id)
            .collect()
    }

    pub fn search_hybrid(
        &self,
        query: &str,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Vec<(&Entry, f32)> {
        let q = query.to_lowercase();
        let q_tokens: Vec<&str> = q.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .filter(|t| !t.is_empty())
            .collect();

        let mut scored: Vec<(&Entry, f32)> = self
            .entries
            .iter()
            .map(|entry| {
                let cos = cosine_similarity(query_embedding, &entry.embedding);
                let mut bonus = 0.0f32;

                if let Some(name) = entry.meta.symbol_name.as_deref() {
                    let n = name.to_lowercase();
                    if q.contains(&n) || n.contains(&q) {
                        bonus += 0.35;
                    }
                    if q_tokens.iter().any(|t| *t == n) {
                        bonus += 0.35;
                    }
                }

                if let Some(sig) = entry.meta.signature.as_deref() {
                    let s = sig.to_lowercase();
                    if q_tokens.iter().any(|t| s.contains(t)) {
                        bonus += 0.15;
                    }
                }

                if let Some(module) = entry.meta.module_path.as_deref() {
                    let m = module.to_lowercase();
                    if q_tokens.iter().any(|t| m.contains(t)) {
                        bonus += 0.10;
                    }
                }

                (entry, cos + bonus)
            })
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Less));
        scored.into_iter().take(top_k).collect()
    }
}
