use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct ChunkId {
    pub hash: [u8; 32],
}

impl ChunkId {
    pub fn to_hex(&self) -> String {
        self.hash.iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        if s.len() != 64 {
            return None;
        }
        let mut hash = [0u8; 32];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let hi = (chunk.get(0)?).to_ascii_lowercase();
            let lo = (chunk.get(1)?).to_ascii_lowercase();
            let hex = [hi, lo];
            let byte = u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?;
            hash[i] = byte;
        }
        Some(ChunkId { hash })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum SymbolType {
    Function,
    Impl,
    ImplMethod,
    Struct,
    Enum,
    Trait,
    Module,
    TypeAlias,
    Const,
    Static,
}


#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChunkKind {
    Paragraph,
    AstNode,
    DocComment,
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
    #[serde(default)]
    pub symbol_type: Option<SymbolType>,
    #[serde(default)]
    pub symbol_name: Option<String>,
    #[serde(default)]
    pub module_path: Option<String>,
    #[serde(default)]
    pub parent_symbol: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub ast_node_type: Option<String>,
    #[serde(default)]
    pub is_public: Option<bool>,
    #[serde(default)]
    pub has_docs: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileFingerprint {
    pub size: u64,
    pub modified: u64,
}
