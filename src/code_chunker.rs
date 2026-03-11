use crate::storage::types::{ChunkKind, ChunkMeta, SymbolKind};
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
        .expect("failed to load Rust grammar");
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
                SymbolKind::Module,
                Some(name),
                None,
                Some(slice_signature(source, node)),
            );
            out.push(CodeChunk {
                span: (node.start_byte(), node.end_byte()),
                meta,
            });
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
                    SymbolKind::ImplMethod,
                    name,
                    parent.clone(),
                    Some(slice_signature(source, child)),
                );
                out.push(CodeChunk {
                    span: (child.start_byte(), child.end_byte()),
                    meta,
                });
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
        );
        out.push(CodeChunk {
            span: (node.start_byte(), node.end_byte()),
            meta,
        });
        return;
    }

    for child in node.children(&mut node.walk()) {
        walk(child, source, file_path, updated_at, module_stack, out);
    }
}

fn map_kind(kind: &str) -> Option<SymbolKind> {
    match kind {
        "function_item" => Some(SymbolKind::Function),
        "struct_item" => Some(SymbolKind::Struct),
        "enum_item" => Some(SymbolKind::Enum),
        "trait_item" => Some(SymbolKind::Trait),
        "type_item" => Some(SymbolKind::TypeAlias),
        "const_item" => Some(SymbolKind::Const),
        "static_item" => Some(SymbolKind::Static),
        _ => None,
    }
}

fn make_meta(
    file_path: &Path,
    updated_at: u64,
    node: Node,
    module_stack: &[String],
    symbol_kind: SymbolKind,
    symbol_name: Option<String>,
    parent_symbol: Option<String>,
    signature: Option<String>,
) -> ChunkMeta {
    let module_path = if module_stack.is_empty() {
        None
    } else {
        Some(module_stack.join("::"))
    };

    ChunkMeta {
        file_path: file_path.to_path_buf(),
        byte_start: node.start_byte() as u64,
        byte_end: node.end_byte() as u64,
        chunk_kind: ChunkKind::AstNode,
        updated_at,
        language: Some("rust".to_string()),
        symbol_kind: Some(symbol_kind),
        symbol_name,
        module_path,
        parent_symbol,
        signature,
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