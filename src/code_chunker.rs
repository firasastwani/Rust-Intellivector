use crate::storage::types::{ChunkKind, ChunkMeta, SymbolType};
use anyhow::Result;
use std::path::Path;
use tree_sitter::{Node, Parser};

pub struct CodeChunk {
    pub meta: ChunkMeta,
    pub span: (usize, usize),
}

pub fn split_rust_ast(
    file_path: &Path,
    source: &[u8],
    updated_at: u64,
) -> Result<Vec<CodeChunk>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("failed to load grammar");
    let tree = parser.parse(source, None).ok_or_else(|| anyhow::anyhow!("parse failed"))?;
    let root = tree.root_node();

    let mut chunks = Vec::new();
    let mut module_stack: Vec<String> = Vec::new();
    walk(
        root,
        source,
        file_path,
        updated_at,
        &mut module_stack,
        &mut chunks,
    );

    Ok(chunks)
}

fn walk(
    node: Node,
    source: &[u8],
    file_path: &Path,
    updated_at: u64,
    module_stack: &mut Vec<String>,
    out: &mut Vec<CodeChunk>,
) {
    let kind = node.kind();

    if kind == "mod_item" {
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = slice_str(source, name_node);
            module_stack.push(name);
        }
        // Optional: create a module chunk
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = slice_str(source, name_node);
            let meta = make_meta(
                file_path,
                updated_at,
                node,
                module_stack,
                SymbolType::Module,
                Some(name),
                None,
                Some(slice_signature(source, node)),
                Some(kind.to_string()),
                source,
            );
            out.push(CodeChunk {
                span: (node.start_byte(), node.end_byte()),
                meta,
            });
            if let Some(doc_chunk) = make_doc_chunk(file_path, updated_at, node, source, module_stack) {
                out.push(doc_chunk);
            }
        }
        for child in node.children(&mut node.walk()) {
            walk(child, source, file_path, updated_at, module_stack, out);
        }
        if node.child_by_field_name("name").is_some() {
            module_stack.pop();
        }
        return;
    }

    if kind == "impl_item" {
        let parent = impl_target_name(node, source);
        let meta = make_meta(
            file_path,
            updated_at,
            node,
            module_stack,
            SymbolType::Impl,
            parent.clone(),
            None,
            Some(slice_signature(source, node)),
            Some(kind.to_string()),
            source,
        );
        out.push(CodeChunk {
            span: (node.start_byte(), node.end_byte()),
            meta,
        });
        if let Some(doc_chunk) = make_doc_chunk(file_path, updated_at, node, source, module_stack) {
            out.push(doc_chunk);
        }
        for child in node.children(&mut node.walk()) {
            if child.kind() == "function_item" {
                let name = child
                    .child_by_field_name("name")
                    .map(|n| slice_str(source, n));
                let meta = make_meta(
                    file_path,
                    updated_at,
                    child,
                    module_stack,
                    SymbolType::ImplMethod,
                    name,
                    parent.clone(),
                    Some(slice_signature(source, child)),
                    Some(child.kind().to_string()),
                    source,
                );
                out.push(CodeChunk {
                    span: (child.start_byte(), child.end_byte()),
                    meta,
                });
                if let Some(doc_chunk) = make_doc_chunk(file_path, updated_at, child, source, module_stack) {
                    out.push(doc_chunk);
                }
            }
        }
        return;
    }

    if let Some(symbol_kind) = map_kind(kind) {
        let name = node
            .child_by_field_name("name")
            .map(|n| slice_str(source, n));
        let meta = make_meta(
            file_path,
            updated_at,
            node,
            module_stack,
            symbol_kind,
            name,
            None,
            Some(slice_signature(source, node)),
            Some(kind.to_string()),
            source,
        );
        out.push(CodeChunk {
            span: (node.start_byte(), node.end_byte()),
            meta,
        });
        if let Some(doc_chunk) = make_doc_chunk(file_path, updated_at, node, source, module_stack) {
            out.push(doc_chunk);
        }
        return;
    }

    for child in node.children(&mut node.walk()) {
        walk(child, source, file_path, updated_at, module_stack, out);
    }
}

fn map_kind(kind: &str) -> Option<SymbolType> {
    match kind {
        "function_item" => Some(SymbolType::Function),
        "struct_item" => Some(SymbolType::Struct),
        "enum_item" => Some(SymbolType::Enum),
        "trait_item" => Some(SymbolType::Trait),
        "type_item" => Some(SymbolType::TypeAlias),
        "const_item" => Some(SymbolType::Const),
        "static_item" => Some(SymbolType::Static),
        _ => None,
    }
}

fn make_meta(
    file_path: &Path,
    updated_at: u64,
    node: Node,
    module_stack: &[String],
    symbol_type: SymbolType,
    symbol_name: Option<String>,
    parent_symbol: Option<String>,
    signature: Option<String>,
    ast_node_type: Option<String>,
    source: &[u8],
) -> ChunkMeta {
    let module_path = if module_stack.is_empty() {
        None
    } else {
        Some(module_stack.join("::"))
    };

    let is_public = is_public(node, source);
    let has_docs = has_docs(node, source);

    ChunkMeta {
        file_path: file_path.to_path_buf(),
        file_hash: 0,
        byte_start: node.start_byte() as u64,
        byte_end: node.end_byte() as u64,
        chunk_kind: ChunkKind::AstNode,
        updated_at,
        language: Some("rust".to_string()),
        symbol_type: Some(symbol_type),
        symbol_name,
        module_path,
        parent_symbol,
        signature,
        ast_node_type,
        is_public: Some(is_public),
        has_docs: Some(has_docs),
    }
}

fn impl_target_name(node: Node, source: &[u8]) -> Option<String> {
    // Try to extract "type" field from impl
    if let Some(ty_node) = node.child_by_field_name("type") {
        return Some(slice_str(source, ty_node));
    }
    None
}

fn slice_signature(source: &[u8], node: Node) -> String {
    if let Some(body) = node.child_by_field_name("body") {
        let start = node.start_byte();
        let end = body.start_byte();
        return String::from_utf8_lossy(&source[start..end]).trim().to_string();
    }
    slice_str(source, node)
}

fn slice_str(source: &[u8], node: Node) -> String {
    let start = node.start_byte();
    let end = node.end_byte();
    String::from_utf8_lossy(&source[start..end]).trim().to_string()
}

fn is_public(node: Node, source: &[u8]) -> bool {
    for child in node.children(&mut node.walk()) {
        if child.kind() == "visibility_modifier" {
            let text = slice_str(source, child);
            if text.starts_with("pub") {
                return true;
            }
        }
    }
    false
}

fn has_docs(node: Node, source: &[u8]) -> bool {
    let mut prev = node.prev_sibling();
    while let Some(sib) = prev {
        if sib.kind() == "line_comment" || sib.kind() == "block_comment" {
            let text = slice_str(source, sib);
            if text.starts_with("///") || text.starts_with("//!") || text.starts_with("/**") {
                return true;
            }
            prev = sib.prev_sibling();
            continue;
        }
        break;
    }
    false
}

fn make_doc_chunk(
    file_path: &Path,
    updated_at: u64,
    node: Node,
    source: &[u8],
    module_stack: &[String],
) -> Option<CodeChunk> {
    let mut start = None;
    let mut end = None;
    let mut prev = node.prev_sibling();
    while let Some(sib) = prev {
        if sib.kind() == "line_comment" || sib.kind() == "block_comment" {
            let text = slice_str(source, sib);
            if text.starts_with("///") || text.starts_with("//!") || text.starts_with("/**") {
                start = Some(sib.start_byte());
                if end.is_none() {
                    end = Some(sib.end_byte());
                }
                prev = sib.prev_sibling();
                continue;
            }
        }
        break;
    }
    let (start, end) = match (start, end) {
        (Some(s), Some(e)) => (s, e),
        _ => return None,
    };

    let module_path = if module_stack.is_empty() {
        None
    } else {
        Some(module_stack.join("::"))
    };
    let symbol_name = node.child_by_field_name("name").map(|n| slice_str(source, n));
    let parent_symbol = if node.kind() == "function_item" {
        None
    } else {
        impl_target_name(node, source)
    };

    Some(CodeChunk {
        span: (start, end),
        meta: ChunkMeta {
            file_path: file_path.to_path_buf(),
            file_hash: 0,
            byte_start: start as u64,
            byte_end: end as u64,
            chunk_kind: ChunkKind::DocComment,
            updated_at,
            language: Some("rust".to_string()),
            symbol_type: None,
            symbol_name,
            module_path,
            parent_symbol,
            signature: None,
            ast_node_type: Some("doc_comment".to_string()),
            is_public: None,
            has_docs: Some(true),
        },
    })
}
