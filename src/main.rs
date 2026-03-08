use crate::embed::Embedder;
use crate::ingest::{map_file, split_chunks};
use crate::store::VectorStore;

use std::fs::File;

mod embed;
mod ingest;
mod similarity;
mod store;

fn main() -> anyhow::Result<()> {
    let embedder = Embedder::load()?;
    println!("Model loaded.");

    let file = File::open("./docs/expl.md").unwrap();
    let mmap = map_file(file);
    let chunks = split_chunks(&mmap, 512);

    let mut store = VectorStore::new();

    for chunk in chunks.iter() {
        let text = std::str::from_utf8(chunk).unwrap();
        let embedding = embedder.embed(text)?;
        store.insert(chunk, embedding);
    }

    println!("Indexed {} chunks.", store.len());

    let first_chunk_text = std::str::from_utf8(chunks[0]).unwrap();
    let query_embedding = embedder.embed(first_chunk_text)?;
    let results = store.search(&query_embedding, 2);

    println!(
        "\nTop results for query: {:?}\n",
        &first_chunk_text[..50.min(first_chunk_text.len())]
    );
    for (i, chunk) in results.iter().enumerate() {
        println!("Result {i}: {}", std::str::from_utf8(chunk).unwrap());
    }

    Ok(())
}
