use crate::embed::Embedder;
use crate::ingest::{map_file, split_chunk_spans, split_code_chunks, wrap_chunk_spans};
use crate::storage::PersistentStore;
use crate::storage::hash::hash_chunk;
use crate::storage::sled_store::SledStore;
use crate::storage::types::{ChunkMeta, FileFingerprint};
use crate::store::VectorStore;
use clap::{Parser, Subcommand};
use std::fs::File;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

mod code_chunker;
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
                    load_file_index_into_store(&db, &mut store, path)?;
                    return query_loop(&embedder, &store);
                }
                db.remove_file_index(path)?;
            }

            let file = File::open(path)?;
            let mmap = map_file(file);

            let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
            let chunks = if ext == "rs" {
                split_code_chunks(path, &mmap, fp.modified)
            } else {
                let spans = split_chunk_spans(&mmap, 512);
                wrap_chunk_spans(path, spans, fp.modified)
            };

            let mut ids = Vec::with_capacity(chunks.len());

            for code_chunk in chunks.iter() {
                let (start, end) = code_chunk.span;
                let chunk_bytes = &mmap[start..end];
                let id = hash_chunk(chunk_bytes);
                ids.push(id);

                let meta = code_chunk.meta.clone();

                let embedding = if let Some(emb) = db.get_embedding(&id)? {
                    emb
                } else {
                    let text = std::str::from_utf8(chunk_bytes)?;
                    let emb = embedder.embed(text)?;
                    db.put_embedding(&id, &emb)?;
                    emb
                };

                db.put_meta(&id, &meta)?;
                store.insert(id, meta, embedding);
            }

            db.set_file_index(path, &ids)?;
            db.set_file_fingerprint(path, &fp)?;

            query_loop(&embedder, &store)?;
        }
    }

    Ok(())
}

fn load_file_index_into_store(
    db: &SledStore,
    store: &mut VectorStore,
    path: &PathBuf,
) -> anyhow::Result<()> {
    let Some(ids) = db.get_file_index(path)? else {
        return Ok(());
    };

    for id in ids {
        let (Some(meta), Some(embedding)) = (db.get_meta(&id)?, db.get_embedding(&id)?) else {
            continue;
        };
        store.insert(id, meta, embedding);
    }

    Ok(())
}

fn query_loop(embedder: &Embedder, store: &VectorStore) -> anyhow::Result<()> {
    println!(
        "Ready ({} chunks). Type a question (or 'exit' to quit):",
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
        let results = store.search_hybrid(question, &query_embedding, 3);

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
