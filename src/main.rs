use crate::ingest::{map_file, split_chunks};

use std::fs::File;

mod ingest;
mod similarity;

fn main() {
    let file = File::open("./docs/expl.md").unwrap();
    let mmap = map_file(file);
    let chunks = split_chunks(&mmap, 512);

    for (i, chunk) in chunks.iter().enumerate() {
        let s = std::str::from_utf8(chunk).unwrap();
        println!("Chunk {i}:\n{s}");
    }
}
