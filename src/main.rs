use crate::context_group::primary_id;
use crate::embed::Embedder;
use crate::search::bm25::Bm25Index;
use crate::storage::sled_store::SledStore;
use crate::store::{
    load_chunk_text, project_hash, read_active_project, write_active_project, VectorStore,
};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

mod code_chunker;
mod context_group;
mod embed;
mod ingest;
mod search;
mod storage;
mod store;

const EMBEDDING_DIM: usize = 384;

#[derive(Parser)]
#[command(name = "code-search")]
#[command(about = "Indexes Rust projects and searches code")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Index { root: PathBuf },
    Update,
    Query { query: String, #[arg(long, default_value_t = 2)] top_k: usize },
    Stats,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match &cli.command {
        Commands::Index { root } => index_project(root),
        Commands::Update => update_project(),
        Commands::Query { query, top_k } => query_project(query, *top_k),
        Commands::Stats => stats_project(),
    }
}

fn project_dir(hash: u64) -> PathBuf {
    Path::new("vector_db").join(format!("{:016x}", hash))
}

fn open_bm25(hash: u64) -> anyhow::Result<Bm25Index> {
    let dir = project_dir(hash).join("bm25");
    Bm25Index::open(&dir)
}

fn open_sled(hash: u64) -> anyhow::Result<SledStore> {
    let dir = project_dir(hash).join("sled");
    SledStore::open(dir)
}

fn index_project(root: &Path) -> anyhow::Result<()> {
    let hash = project_hash(root)?;
    let db = open_sled(hash)?;
    let mut bm25 = open_bm25(hash)?;
    let embedder = Embedder::load()?;
    let mut store = VectorStore::new(root.to_path_buf(), EMBEDDING_DIM);
    store.index_project(root, &embedder, &db, &mut bm25)?;
    write_active_project(root, hash)?;
    println!("Indexed project at {:?}", root);
    Ok(())
}

fn update_project() -> anyhow::Result<()> {
    let (hash, root) = read_active_project()?;
    let db = open_sled(hash)?;
    let mut bm25 = open_bm25(hash)?;
    let embedder = Embedder::load()?;
    let mut store = VectorStore::load_from_store(&db, root.clone(), EMBEDDING_DIM)?;
    store.update_project(&root, &embedder, &db, &mut bm25)?;
    println!("Updated index for {:?}", root);
    Ok(())
}

fn query_project(query: &str, top_k: usize) -> anyhow::Result<()> {
    let (hash, root) = read_active_project()?;
    let db = open_sled(hash)?;
    let bm25 = open_bm25(hash)?;
    let embedder = Embedder::load()?;
    let store = VectorStore::load_from_store(&db, root.clone(), EMBEDDING_DIM)?;

    let results = store.search_groups(query, &embedder, &bm25, top_k)?;

    println!("\nTop results for: \"{}\"\n", query);
    for (i, result) in results.iter().enumerate() {
        let primary = primary_id(&result.group);
        let primary_meta = store.entries().iter().find(|e| e.id == primary).map(|e| &e.meta);
        println!("--- Result {} ---", i + 1);
        println!("Score: {:.4}", result.score);
        if let Some(meta) = primary_meta {
            let symbol = meta.symbol_name.as_deref().unwrap_or("[unknown]");
            let symbol_type = meta
                .symbol_type
                .as_ref()
                .map(|s| format!("{:?}", s))
                .unwrap_or_else(|| "Unknown".to_string());
            println!(
                "Symbol: {} ({})\nFile: {}",
                symbol,
                symbol_type,
                meta.file_path.to_string_lossy()
            );
        }

        let mut members = crate::context_group::member_ids(&result.group);
        members.sort_by(|a, b| {
            let ma = store.entries().iter().find(|e| e.id == *a).map(|e| &e.meta);
            let mb = store.entries().iter().find(|e| e.id == *b).map(|e| &e.meta);
            match (ma, mb) {
                (Some(ma), Some(mb)) => {
                    let pa = ma.file_path.to_string_lossy();
                    let pb = mb.file_path.to_string_lossy();
                    let file_cmp = pa.cmp(&pb);
                    if file_cmp == std::cmp::Ordering::Equal {
                        ma.byte_start.cmp(&mb.byte_start)
                    } else {
                        file_cmp
                    }
                }
                _ => std::cmp::Ordering::Equal,
            }
        });
        members.dedup();

        for id in members {
            if let Some(meta) = store.entries().iter().find(|e| e.id == id).map(|e| &e.meta) {
                let text = load_chunk_text(&root, meta)?;
                println!("{}\n", text);
            }
        }
    }
    Ok(())
}

fn stats_project() -> anyhow::Result<()> {
    let (hash, root) = read_active_project()?;
    let db = open_sled(hash)?;
    let store = VectorStore::load_from_store(&db, root.clone(), EMBEDDING_DIM)?;
    let (files, chunks) = store.stats();
    println!("Project root: {:?}", root);
    println!("Files indexed: {}", files);
    println!("Chunks indexed: {}", chunks);
    Ok(())
}
