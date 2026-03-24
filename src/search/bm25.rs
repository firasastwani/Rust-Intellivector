use crate::storage::types::{ChunkId, ChunkMeta};
use anyhow::Result;
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Schema, TEXT, STORED, STRING};
use tantivy::{Index, IndexReader, IndexWriter, Term};
use tantivy::schema::OwnedValue;

pub struct Bm25Index {
    index: Index,
    reader: IndexReader,
    writer: IndexWriter,
    fields: Bm25Fields,
}

#[derive(Clone, Copy)]
pub struct Bm25Fields {
    pub doc_id: tantivy::schema::Field,
    pub file_path: tantivy::schema::Field,
    pub module_path: tantivy::schema::Field,
    pub symbol_name: tantivy::schema::Field,
    pub signature: tantivy::schema::Field,
    pub language: tantivy::schema::Field,
    pub search_text: tantivy::schema::Field,
}

impl Bm25Index {
    pub fn open(path: &Path) -> Result<Self> {
        let mut schema_builder = Schema::builder();
        let doc_id = schema_builder.add_text_field("doc_id", STRING | STORED);
        let file_path = schema_builder.add_text_field("file_path", STRING | STORED);
        let module_path = schema_builder.add_text_field("module_path", TEXT | STORED);
        let symbol_name = schema_builder.add_text_field("symbol_name", TEXT | STORED);
        let signature = schema_builder.add_text_field("signature", TEXT | STORED);
        let language = schema_builder.add_text_field("language", STRING | STORED);
        let search_text = schema_builder.add_text_field("search_text", TEXT);
        let schema = schema_builder.build();

        std::fs::create_dir_all(path)?;
        let index = Index::open_or_create(tantivy::directory::MmapDirectory::open(path)?, schema.clone())?;
        let reader = index
            .reader_builder()
            .reload_policy(tantivy::ReloadPolicy::OnCommitWithDelay)
            .try_into()?;
        let writer = index.writer(50_000_000)?;

        Ok(Self {
            index,
            reader,
            writer,
            fields: Bm25Fields {
                doc_id,
                file_path,
                module_path,
                symbol_name,
                signature,
                language,
                search_text,
            },
        })
    }

    pub fn index_chunk(&mut self, id: ChunkId, meta: &ChunkMeta, search_text: &str) -> Result<()> {
        let mut doc = tantivy::schema::TantivyDocument::default();
        doc.add_text(self.fields.doc_id, &id.to_hex());
        doc.add_text(self.fields.file_path, &meta.file_path.to_string_lossy());
        if let Some(module_path) = meta.module_path.as_deref() {
            doc.add_text(self.fields.module_path, module_path);
        }
        if let Some(symbol_name) = meta.symbol_name.as_deref() {
            doc.add_text(self.fields.symbol_name, symbol_name);
        }
        if let Some(signature) = meta.signature.as_deref() {
            doc.add_text(self.fields.signature, signature);
        }
        if let Some(language) = meta.language.as_deref() {
            doc.add_text(self.fields.language, language);
        }
        doc.add_text(self.fields.search_text, search_text);
        self.writer.add_document(doc)?;
        Ok(())
    }

    pub fn remove_file(&mut self, path: &Path) -> Result<()> {
        let term = Term::from_field_text(self.fields.file_path, &path.to_string_lossy());
        self.writer.delete_term(term);
        Ok(())
    }

    pub fn commit(&mut self) -> Result<()> {
        self.writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn search(&self, query: &str, top_k: usize) -> Result<Vec<ChunkId>> {
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(
            &self.index,
            vec![
                self.fields.search_text,
                self.fields.symbol_name,
                self.fields.signature,
                self.fields.module_path,
                self.fields.file_path,
            ],
        );
        let q = parser.parse_query(query)?;
        let top_docs = searcher.search(&q, &TopDocs::with_limit(top_k))?;
        let mut out = Vec::new();
        for (_score, addr) in top_docs {
            let retrieved = searcher.doc::<tantivy::schema::TantivyDocument>(addr)?;
            let id_val = retrieved
                .get_first(self.fields.doc_id)
                .and_then(owned_value_to_str)
                .and_then(ChunkId::from_hex);
            if let Some(id) = id_val {
                out.push(id);
            }
        }
        Ok(out)
    }
}

fn owned_value_to_str(value: &OwnedValue) -> Option<&str> {
    match value {
        OwnedValue::Str(s) => Some(s.as_str()),
        OwnedValue::PreTokStr(s) => Some(s.text.as_str()),
        _ => None,
    }
}
