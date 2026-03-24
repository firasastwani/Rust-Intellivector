use crate::storage::types::ChunkId;
use std::collections::HashMap;

pub fn rrf_fuse(
    ranked_lists: &[Vec<ChunkId>],
    k: f32,
) -> HashMap<ChunkId, f32> {
    let mut scores: HashMap<ChunkId, f32> = HashMap::new();
    for list in ranked_lists {
        for (idx, chunk_id) in list.iter().enumerate() {
            let rank = (idx + 1) as f32;
            let entry = scores.entry(*chunk_id).or_insert(0.0);
            *entry += 1.0 / (k + rank);
        }
    }
    scores
}
