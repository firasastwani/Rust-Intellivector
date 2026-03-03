use crate::ingest::{map_file, split_chunks};
use crate::store::VectorStore;

use std::fs::File;

mod ingest;
mod similarity;
mod store;

fn fake_embedding(seed: usize, dim: usize) -> Vec<f32> {
    (0..dim).map(|i| ((seed + i) as f32 * 0.1).sin()).collect()
}

fn main() {
    let file = File::open("./docs/expl.md").unwrap();
    let mmap = map_file(file);
    let chunks = split_chunks(&mmap, 512);

    let dim = 8;
    let mut store = VectorStore::new();

    for (i, chunk) in chunks.iter().enumerate() {
        store.insert(chunk, fake_embedding(i, dim));
    }

    let query = fake_embedding(0, dim); // should be the highest against chunk 0
    let results = store.search(&query, 2);

    for (i, chunk) in results.iter().enumerate() {
        println!("Result {i}: {}", std::str::from_utf8(chunk).unwrap());
    }
}
