use memmap2::Mmap;
use std::fs::File;
use std::path::PathBuf; 

use crate::code_chunker::*; 
use crate::storage::types::*; 

#[derive(Debug, Clone, Copy)]
pub struct ChunkSpan {
    pub start: usize,
    pub end: usize,
}


// memory-maps the file; caller holds the Mmap so chunk slices can borrow it
pub fn map_file(file: File) -> Mmap {
    let mmap = unsafe { Mmap::map(&file) }.unwrap();
    mmap.advise(memmap2::Advice::Sequential).unwrap();
    mmap
}

// splits input into chunk spans of either `chunk_size` chars or a new paragraph.
// spans are byte offsets into the provided data.
pub fn split_chunk_spans(data: &[u8], chunk_size: usize) -> Vec<ChunkSpan> {
    let chars_to_bytes = chunk_size * 4; // UTF-8 max 4 bytes per char

    let mut spans: Vec<ChunkSpan> = Vec::new();

    let mut start: usize = 0;

    while start < data.len() {
        let mut end = start;

        while end < data.len()
            && end - start < chars_to_bytes
            && !(end - start >= 2 && data[end - 2..end] == *b"\n\n")
        {
            end += 1;
        }

        spans.push(ChunkSpan { start, end });
        start = end;
    }

    spans
}

pub fn wrap_chunk_spans( file_path: &PathBuf,
    spans: Vec<ChunkSpan>,
    updated_at: u64,
) -> Vec<CodeChunk> {
    spans
        .into_iter()
        .map(|span| CodeChunk {
            meta: ChunkMeta {
                file_path: file_path.to_path_buf(),
                byte_start: span.start as u64,
                byte_end: span.end as u64,
                chunk_kind: ChunkKind::Paragraph, // or Text
                updated_at,
                language: None,
                symbol_type: None,
                symbol_name: None,
                module_path: None,
                parent_symbol: None,
                signature: None,
                ast_node_type: None,
                is_public: None,
                has_docs: None,
            },
            span: (span.start, span.end),
        })
        .collect()
}

pub fn split_code_chunks(path: &PathBuf, data: &[u8], updated_at: u64) -> Vec<CodeChunk>{
    split_rust_ast(path, data, updated_at).unwrap() 
}
