use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ChunkId {
    pub hash: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SymbolKind {
    Function, 
    ImplMethod, 
    Struct,
    Enum,
    Trait, 
    Module,
    TypeAlias,
    Const,
    Static,
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ChunkKind {
    Paragraph,
    AstNode,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkMeta {
    pub file_path: PathBuf,
    pub byte_start: u64,
    pub byte_end: u64,
    pub chunk_kind: ChunkKind,
    pub updated_at: u64,
    pub language: Option<String>,

    // code specifc meta data
    pub symbol_kind: Option<SymbolKind>,
    pub symbol_name: Option<String>,
    pub module_path: Option<String>,
    pub parent_symbol: Option<String>,
    pub signature: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileFingerprint {
    pub size: u64,
    pub modified: u64,
}

