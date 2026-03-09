use crate::embed::Embedder;
use crate::ingest::{map_file, split_chunks};
use crate::store::VectorStore;
use clap::{Parser, Subcommand};
use std::fs::File;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

mod embed;
mod ingest;
mod similarity;
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

    let cli = Cli::parse();

    match &cli.command {
        Commands::Ingest { path } => {
            let file = File::open(path)?;
            let mmap = map_file(file);
            let chunks = split_chunks(&mmap, 512);

            for chunk in chunks.iter() {
                let text = std::str::from_utf8(chunk)?;
                let embedding = embedder.embed(text)?;
                store.insert(chunk, embedding);
            }

            println!("Indexed {} chunks. Type a question (or 'exit' to quit):", store.len());

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

                println!("\nTop results for: \"{}\"\n", &question[..50.min(question.len())]);
                for (i, chunk) in results.iter().enumerate() {
                    println!("--- Result {} ---", i + 1);
                    println!("{}\n", std::str::from_utf8(chunk)?);
                }
            }
        }
    }

    Ok(())
}
