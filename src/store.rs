use crate::similarity::cosine_similarity;

// can i just literally everything as borrowed u8s and never copy stuff?
pub struct VectorStore<'a> {
    entries: Vec<(&'a [u8], Vec<f32>)>,
}

impl<'a> VectorStore<'a> {
    pub fn new() -> Self {
        VectorStore {
            entries: Vec::new(),
        }
    }

    pub fn insert(&mut self, chunk: &'a [u8], embedding: Vec<f32>) {
        self.entries.push((chunk, embedding));
    }

    pub fn search(&self, query_embedding: &[f32], top_k: usize) -> Vec<&[u8]> {
        let mut scored: Vec<(f32, &[u8])> = self
            .entries
            .iter()
            .map(|(chunk, emb)| (cosine_similarity(query_embedding, emb), *chunk))
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Less));

        scored
            .into_iter()
            .take(top_k)
            .map(|(_, chunk)| chunk)
            .collect()
    }
}
