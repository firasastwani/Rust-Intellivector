use crate::storage::types::ChunkId;

pub fn hash_chunk(bytes: &[u8]) -> ChunkId {
    let hash = blake3::hash(bytes);
    ChunkId {
        hash: *hash.as_bytes(),
    }
}
