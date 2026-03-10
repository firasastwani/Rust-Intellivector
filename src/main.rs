use crate::embed::Embedder;
use crate::ingest::{map_file, split_chunk_spans};
use crate::storage::hash::hash_chunk;
use crate::storage::sled_store::SledStore;
use crate::storage::types::{ChunkKind, ChunkMeta, FileFingerprint};
use crate::storage::PersistentStore;
use crate::store::VectorStore;
use clap::{Parser, Subcommand};
use std::fs::File;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

mod embed;
mod ingest;
mod similarity;
mod storage;
mod store;

#[derive(Parser)]
#[command(name = "VectorTool")]
#[command(
    about = "Reads either a .txt or .md file and stores its Vector embeddings then answers queries"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Ingest { path: PathBuf },
}

fn main() -> anyhow::Result<()> {
    let embedder = Embedder::load()?;
    let mut store = VectorStore::new();
    let db = SledStore::open("vector_db")?;

    let cli = Cli::parse();

    match &cli.command {
        Commands::Ingest { path } => {
            let fp = file_fingerprint(path)?;
            if let Some(prev) = db.get_file_fingerprint(path)? {
                if prev == fp {
                    println!("No changes detected for {:?}", path);
                    return Ok(());
                }
                db.remove_file_index(path)?;
            }

            let file = File::open(path)?;
            let mmap = map_file(file);
            let spans = split_chunk_spans(&mmap, 512);

            let mut ids: Vec<_> = Vec::with_capacity(spans.len());

            for span in spans.iter() {
                let chunk = &mmap[span.start..span.end];
                let id = hash_chunk(chunk);
                ids.push(id);

                let meta = ChunkMeta {
                    file_path: path.clone(),
                    byte_start: span.start as u64,
                    byte_end: span.end as u64,
                    chunk_kind: ChunkKind::Paragraph,
                    updated_at: fp.modified,
                    language: None,
                };

                let embedding = if let Some(emb) = db.get_embedding(&id)? {
                    emb
                } else {
                    let text = std::str::from_utf8(chunk)?;
                    let emb = embedder.embed(text)?;
                    db.put_embedding(&id, &emb)?;
                    emb
                };

                db.put_meta(&id, &meta)?;
                store.insert(id, meta, embedding);
            }

            db.set_file_index(path, &ids)?;
            db.set_file_fingerprint(path, &fp)?;

            println!(
                "Indexed {} chunks. Type a question (or 'exit' to quit):",
                store.len()
            );

            let stdin = io::stdin();
            loop {
                print!("> ");
                io::stdout().flush()?;

                let mut line = String::new();
                stdin.lock().read_line(&mut line)?;
                let question = line.trim();

                if question == "exit" || question.is_empty() {
                    break;
                }

                let query_embedding = embedder.embed(question)?;
                let results = store.search(&query_embedding, 3);

                println!(
                    "\nTop results for: \"{}\"\n",
                    &question[..50.min(question.len())]
                );
                for (i, (entry, score)) in results.iter().enumerate() {
                    println!("--- Result {} ---", i + 1);
                    println!("Score: {:.4}", score);
                    let text = load_chunk_text(&entry.meta)?;
                    println!("{}\n", text);
                }
            }
        }
    }

    Ok(())
}

fn file_fingerprint(path: &PathBuf) -> anyhow::Result<FileFingerprint> {
    let meta = std::fs::metadata(path)?;
    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let modified = modified
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    Ok(FileFingerprint {
        size: meta.len(),
        modified,
    })
}

fn load_chunk_text(meta: &ChunkMeta) -> anyhow::Result<String> {
    let bytes = std::fs::read(&meta.file_path)?;
    let start = meta.byte_start as usize;
    let end = meta.byte_end as usize;
    if start >= bytes.len() || end > bytes.len() || start >= end {
        return Ok(String::from("[invalid chunk range]"));
    }
    Ok(String::from_utf8_lossy(&bytes[start..end]).to_string())
}
