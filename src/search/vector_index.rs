use crate::storage::types::ChunkId;
use hnsw_rs::prelude::*;

pub struct VectorIndex {
    hnsw: Hnsw<'static, f32, DistCosine>,
    chunk_ids: Vec<ChunkId>,
}

impl VectorIndex {
    pub fn new(dimension: usize) -> Self {
        let hnsw = Hnsw::new(16, dimension, 16, 10_000, DistCosine {});
        Self {
            hnsw,
            chunk_ids: Vec::new(),
        }
    }

    pub fn insert(&mut self, chunk_id: ChunkId, embedding: &[f32]) {
        let idx = self.chunk_ids.len();
        self.chunk_ids.push(chunk_id);
        self.hnsw.insert((embedding, idx));
    }

    pub fn rebuild(&mut self, dimension: usize, items: &[(ChunkId, Vec<f32>)]) {
        *self = VectorIndex::new(dimension);
        for (id, emb) in items {
            self.insert(*id, emb);
        }
    }

    pub fn search(&self, query: &[f32], k: usize) -> Vec<(ChunkId, f32)> {
        self.hnsw
            .search(query, k, 50)
            .into_iter()
            .filter_map(|neighbor| {
                let chunk_id = self.chunk_ids.get(neighbor.d_id)?.clone();
                let score = 1.0 - neighbor.distance;
                Some((chunk_id, score))
            })
            .collect()
    }
}
