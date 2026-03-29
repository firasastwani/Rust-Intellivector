use crate::storage::types::{ChunkId, ChunkKind, ChunkMeta, SymbolType};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum ContextGroup {
    Loose {
        chunk: ChunkId,
    },
    Function {
        signature: ChunkId,
        body: Vec<ChunkId>,
        docs: Option<ChunkId>,
    },
    Module {
        declaration: ChunkId,
        items: Vec<ChunkId>,
    },
    Impl {
        declaration: ChunkId,
        methods: Vec<ChunkId>,
        docs: Option<ChunkId>,
    },
    Struct {
        declaration: ChunkId,
        fields: Vec<ChunkId>,
        impls: Vec<ChunkId>,
        docs: Option<ChunkId>,
    },
    Enum {
        declaration: ChunkId,
        variants: Vec<ChunkId>,
        impls: Vec<ChunkId>,
        docs: Option<ChunkId>,
    },
    Trait {
        declaration: ChunkId,
        items: Vec<ChunkId>,
        impls: Vec<ChunkId>,
        docs: Option<ChunkId>,
    },
}

#[derive(Debug, Clone)]
pub struct GroupIndex {
    pub groups: Vec<ContextGroup>,
}

#[derive(Debug, Clone)]
pub struct GroupResult {
    pub group: ContextGroup,
    pub score: f32,
}

pub fn build_groups(chunks: &[(ChunkId, ChunkMeta)]) -> GroupIndex {
    let mut groups: Vec<ContextGroup> = Vec::new();
    let mut grouped: HashMap<ChunkId, usize> = HashMap::new();

    let mut docs_by_symbol: HashMap<String, ChunkId> = HashMap::new();
    for (id, meta) in chunks {
        if meta.chunk_kind == ChunkKind::DocComment {
            if let Some(name) = meta.symbol_name.as_deref() {
                docs_by_symbol.insert(name.to_string(), *id);
            } else if let Some(parent) = meta.parent_symbol.as_deref() {
                docs_by_symbol.insert(parent.to_string(), *id);
            }
        }
    }

    let mut impls_by_target: HashMap<String, Vec<ChunkId>> = HashMap::new();
    let mut impl_methods_by_target: HashMap<String, Vec<ChunkId>> = HashMap::new();
    let mut modules_by_path: HashMap<String, Vec<ChunkId>> = HashMap::new();

    for (id, meta) in chunks {
        if let Some(SymbolType::Impl) = meta.symbol_type {
            if let Some(target) = meta.symbol_name.as_deref() {
                impls_by_target.entry(target.to_string()).or_default().push(*id);
            }
        }
        if let Some(SymbolType::ImplMethod) = meta.symbol_type {
            if let Some(target) = meta.parent_symbol.as_deref() {
                impl_methods_by_target.entry(target.to_string()).or_default().push(*id);
            }
        }
        if let Some(module_path) = meta.module_path.as_deref() {
            modules_by_path
                .entry(module_path.to_string())
                .or_default()
                .push(*id);
        }
    }

    for (id, meta) in chunks {
        match meta.symbol_type {
            Some(SymbolType::Function) => {
                let docs = meta
                    .symbol_name
                    .as_deref()
                    .and_then(|n| docs_by_symbol.get(n).copied());
                let group = ContextGroup::Function {
                    signature: *id,
                    body: Vec::new(),
                    docs,
                };
                let idx = groups.len();
                groups.push(group);
                grouped.insert(*id, idx);
                if let Some(doc_id) = docs {
                    grouped.insert(doc_id, idx);
                }
            }
            Some(SymbolType::Impl) => {
                let methods = meta
                    .symbol_name
                    .as_deref()
                    .and_then(|n| impl_methods_by_target.get(n).cloned())
                    .unwrap_or_default();
                let docs = meta
                    .symbol_name
                    .as_deref()
                    .and_then(|n| docs_by_symbol.get(n).copied());
                let group = ContextGroup::Impl {
                    declaration: *id,
                    methods: methods.clone(),
                    docs,
                };
                let idx = groups.len();
                groups.push(group);
                grouped.insert(*id, idx);
                for m in methods {
                    grouped.insert(m, idx);
                }
                if let Some(doc_id) = docs {
                    grouped.insert(doc_id, idx);
                }
            }
            Some(SymbolType::Struct) => {
                let impls = meta
                    .symbol_name
                    .as_deref()
                    .and_then(|n| impls_by_target.get(n).cloned())
                    .unwrap_or_default();
                let docs = meta
                    .symbol_name
                    .as_deref()
                    .and_then(|n| docs_by_symbol.get(n).copied());
                let group = ContextGroup::Struct {
                    declaration: *id,
                    fields: Vec::new(),
                    impls: impls.clone(),
                    docs,
                };
                let idx = groups.len();
                groups.push(group);
                grouped.insert(*id, idx);
                for imp in impls {
                    grouped.insert(imp, idx);
                }
                if let Some(doc_id) = docs {
                    grouped.insert(doc_id, idx);
                }
            }
            Some(SymbolType::Enum) => {
                let impls = meta
                    .symbol_name
                    .as_deref()
                    .and_then(|n| impls_by_target.get(n).cloned())
                    .unwrap_or_default();
                let docs = meta
                    .symbol_name
                    .as_deref()
                    .and_then(|n| docs_by_symbol.get(n).copied());
                let group = ContextGroup::Enum {
                    declaration: *id,
                    variants: Vec::new(),
                    impls: impls.clone(),
                    docs,
                };
                let idx = groups.len();
                groups.push(group);
                grouped.insert(*id, idx);
                for imp in impls {
                    grouped.insert(imp, idx);
                }
                if let Some(doc_id) = docs {
                    grouped.insert(doc_id, idx);
                }
            }
            Some(SymbolType::Trait) => {
                let impls = meta
                    .symbol_name
                    .as_deref()
                    .and_then(|n| impls_by_target.get(n).cloned())
                    .unwrap_or_default();
                let docs = meta
                    .symbol_name
                    .as_deref()
                    .and_then(|n| docs_by_symbol.get(n).copied());
                let group = ContextGroup::Trait {
                    declaration: *id,
                    items: Vec::new(),
                    impls: impls.clone(),
                    docs,
                };
                let idx = groups.len();
                groups.push(group);
                grouped.insert(*id, idx);
                for imp in impls {
                    grouped.insert(imp, idx);
                }
                if let Some(doc_id) = docs {
                    grouped.insert(doc_id, idx);
                }
            }
            Some(SymbolType::Module) => {
                let items = full_module_path(meta)
                    .and_then(|p| modules_by_path.get(&p).cloned())
                    .unwrap_or_default();
                let group = ContextGroup::Module {
                    declaration: *id,
                    items: items.clone(),
                };
                let idx = groups.len();
                groups.push(group);
                grouped.insert(*id, idx);
                for item in items {
                    grouped.insert(item, idx);
                }
            }
            _ => {}
        }
    }

    for (id, _) in chunks {
        if grouped.contains_key(id) {
            continue;
        }
        let idx = groups.len();
        groups.push(ContextGroup::Loose { chunk: *id });
        grouped.insert(*id, idx);
    }

    GroupIndex {
        groups,
    }
}

pub fn rank_groups(
    group_index: &GroupIndex,
    metas: &HashMap<ChunkId, ChunkMeta>,
    chunk_scores: &HashMap<ChunkId, f32>,
    top_k: usize,
) -> Vec<GroupResult> {
    let mut scored: Vec<GroupResult> = Vec::new();

    for group in &group_index.groups {
        let member_ids = member_ids(group);
        let mut scores: Vec<f32> = member_ids
            .iter()
            .filter_map(|id| chunk_scores.get(id).copied())
            .collect();
        if scores.is_empty() {
            continue;
        }
        scores.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let max_score = scores[0];
        let mean_top2 = if scores.len() >= 2 {
            (scores[0] + scores[1]) / 2.0
        } else {
            scores[0]
        };
        let primary = primary_id(group);
        let (has_docs, is_public) = metas
            .get(&primary)
            .map(|m| (m.has_docs.unwrap_or(false), m.is_public.unwrap_or(false)))
            .unwrap_or((false, false));
        let mut score = 0.6 * max_score + 0.3 * mean_top2;
        if has_docs {
            score += 0.1;
        }
        if is_public {
            score += 0.1;
        }
        scored.push(GroupResult {
            group: group.clone(),
            score,
        });
    }

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);
    scored
}

pub fn member_ids(group: &ContextGroup) -> Vec<ChunkId> {
    match group {
        ContextGroup::Loose { chunk } => vec![*chunk],
        ContextGroup::Function { signature, body, docs } => {
            let mut ids = vec![*signature];
            ids.extend(body.iter().copied());
            if let Some(d) = docs {
                ids.push(*d);
            }
            ids
        }
        ContextGroup::Module { declaration, items } => {
            let mut ids = vec![*declaration];
            ids.extend(items.iter().copied());
            ids
        }
        ContextGroup::Impl { declaration, methods, docs } => {
            let mut ids = vec![*declaration];
            ids.extend(methods.iter().copied());
            if let Some(d) = docs {
                ids.push(*d);
            }
            ids
        }
        ContextGroup::Struct { declaration, fields, impls, docs } => {
            let mut ids = vec![*declaration];
            ids.extend(fields.iter().copied());
            ids.extend(impls.iter().copied());
            if let Some(d) = docs {
                ids.push(*d);
            }
            ids
        }
        ContextGroup::Enum { declaration, variants, impls, docs } => {
            let mut ids = vec![*declaration];
            ids.extend(variants.iter().copied());
            ids.extend(impls.iter().copied());
            if let Some(d) = docs {
                ids.push(*d);
            }
            ids
        }
        ContextGroup::Trait { declaration, items, impls, docs } => {
            let mut ids = vec![*declaration];
            ids.extend(items.iter().copied());
            ids.extend(impls.iter().copied());
            if let Some(d) = docs {
                ids.push(*d);
            }
            ids
        }
    }
}

pub fn primary_id(group: &ContextGroup) -> ChunkId {
    match group {
        ContextGroup::Loose { chunk } => *chunk,
        ContextGroup::Function { signature, .. } => *signature,
        ContextGroup::Module { declaration, .. } => *declaration,
        ContextGroup::Impl { declaration, .. } => *declaration,
        ContextGroup::Struct { declaration, .. } => *declaration,
        ContextGroup::Enum { declaration, .. } => *declaration,
        ContextGroup::Trait { declaration, .. } => *declaration,
    }
}

fn full_module_path(meta: &ChunkMeta) -> Option<String> {
    let name = meta.symbol_name.as_deref()?;
    if let Some(parent) = meta.module_path.as_deref() {
        Some(format!("{}::{}", parent, name))
    } else {
        Some(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::types::{ChunkKind, SymbolType};
    use std::path::PathBuf;

    fn meta(symbol_type: Option<SymbolType>, name: Option<&str>) -> ChunkMeta {
        ChunkMeta {
            file_path: PathBuf::from("src/lib.rs"),
            file_hash: 0,
            byte_start: 0,
            byte_end: 1,
            chunk_kind: ChunkKind::AstNode,
            updated_at: 0,
            language: Some("rust".to_string()),
            symbol_type,
            symbol_name: name.map(|s| s.to_string()),
            module_path: None,
            parent_symbol: None,
            signature: None,
            ast_node_type: Some("function_item".to_string()),
            is_public: Some(true),
            has_docs: Some(false),
        }
    }

    #[test]
    fn groups_function_with_docs() {
        let f_id = ChunkId { hash: [1u8; 32] };
        let d_id = ChunkId { hash: [2u8; 32] };
        let f_meta = meta(Some(SymbolType::Function), Some("foo"));
        let mut d_meta = meta(None, Some("foo"));
        d_meta.chunk_kind = ChunkKind::DocComment;
        d_meta.ast_node_type = Some("doc_comment".to_string());
        let groups = build_groups(&[(f_id, f_meta), (d_id, d_meta)]);
        assert_eq!(groups.groups.len(), 1);
        assert!(matches!(groups.groups[0], ContextGroup::Function { .. }));
    }

    #[test]
    fn groups_loose_for_ungrouped_chunk() {
        let id = ChunkId { hash: [9u8; 32] };
        let meta = meta(None, None);
        let groups = build_groups(&[(id, meta)]);
        assert_eq!(groups.groups.len(), 1);
        assert!(matches!(groups.groups[0], ContextGroup::Loose { .. }));
    }
}
