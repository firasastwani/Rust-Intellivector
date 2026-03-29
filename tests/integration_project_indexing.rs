use mem_system::storage::sled_store::SledStore;
use mem_system::storage::types::{ChunkId, ChunkKind, ChunkMeta};
use mem_system::store::{hash_file, VectorStore};
use mem_system::search::bm25::Bm25Index;
use mem_system::storage::PersistentStore;
use std::path::{Path, PathBuf};

fn temp_dir(name: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("mem_system_test_{}_{}", name, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_meta(rel_path: &Path) -> ChunkMeta {
    ChunkMeta {
        file_path: rel_path.to_path_buf(),
        file_hash: 1,
        byte_start: 0,
        byte_end: 1,
        chunk_kind: ChunkKind::Paragraph,
        updated_at: 0,
        language: None,
        symbol_type: None,
        symbol_name: None,
        module_path: None,
        parent_symbol: None,
        signature: None,
        ast_node_type: None,
        is_public: None,
        has_docs: None,
    }
}

#[test]
fn hash_file_changes_on_update() {
    let dir = temp_dir("hash");
    let path = dir.join("file.rs");
    std::fs::write(&path, "fn a() {}").unwrap();
    let first = hash_file(&path).unwrap();
    std::fs::write(&path, "fn b() {}").unwrap();
    let second = hash_file(&path).unwrap();
    assert_ne!(first, second);
}

#[test]
fn remove_chunks_for_file_clears_storage() {
    let dir = temp_dir("remove");
    let sled_dir = dir.join("sled");
    let bm25_dir = dir.join("bm25");
    std::fs::create_dir_all(&bm25_dir).unwrap();

    let db = SledStore::open(&sled_dir).unwrap();
    let mut bm25 = Bm25Index::open(&bm25_dir).unwrap();

    let rel_path = Path::new("src/lib.rs");
    let id = ChunkId { hash: [7u8; 32] };
    let meta = make_meta(rel_path);
    let embedding = vec![0.0f32; 384];

    db.put_meta(&id, &meta).unwrap();
    db.put_embedding(&id, &embedding).unwrap();
    db.set_file_index(rel_path, &[id]).unwrap();
    db.set_file_hash(rel_path, 42).unwrap();
    bm25.index_chunk(id, &meta, "test").unwrap();
    bm25.commit().unwrap();

    let mut store = VectorStore::new(PathBuf::from("."), 384);

    store.remove_chunks_for_file(rel_path, &db, &mut bm25).unwrap();

    assert!(db.get_meta(&id).unwrap().is_none());
    assert!(db.get_embedding(&id).unwrap().is_none());
    assert!(db.get_file_index(rel_path).unwrap().is_none());
    assert_eq!(store.entries().len(), 0);
}
