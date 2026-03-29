# Parallel + Batched Embedding Plan

## Summary
- Add batched embedding and parallel file indexing.
- Use auto-heuristic batch sizing.
- Keep semantic embeddings always on.

## Step 1 — Add `embed_batch`
**File:** `src/embed.rs`
- Add:
  ```rust
  pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>
  ```
- Implementation:
  1. Tokenize all texts with truncation to 512.
  2. Pad token IDs + attention masks to max length in batch.
  3. Build tensors `[batch, seq_len]`.
  4. Forward pass once.
  5. Mean-pool per row and L2-normalize.

## Step 2 — Batch Size Heuristic
**File:** `src/store.rs` (or new helper module)
- Add:
  ```rust
  fn choose_batch_size(max_tokens: usize) -> usize
  ```
- Suggested heuristic:
  - `> 384` → 4
  - `> 256` → 8
  - else → 16

## Step 3 — Build File Entries in Batch
**File:** `src/store.rs`
- Add:
  ```rust
  struct IndexEntry {
      id: ChunkId,
      meta: ChunkMeta,
      embedding: Vec<f32>,
      bm25_text: String,
  }
  ```
- Add:
  ```rust
  fn build_entries_for_file(...) -> Result<Vec<IndexEntry>>
  ```
- Implementation:
  - chunk file
  - collect chunk texts
  - call `embed_batch`
  - build `IndexEntry` list

## Step 4 — Parallel File Indexing
**File:** `src/store.rs`
- In `index_project`:
  - Use Rayon: `files.par_iter()`
  - For each file, call `build_entries_for_file` **without writing to sled**
  - Collect all entries in memory
- Sequentially write to:
  - sled meta/embeddings
  - bm25 index
  - vector index

## Step 5 — Update Project Incrementally
- For changed files:
  - remove old chunks (sequential)
  - rebuild entries (parallel prepare)
  - commit new chunks (sequential)
- If many changes, call `rebuild_vector_index()`

## Step 6 — Tests
- Unit tests:
  - `embed_batch` count + vector size
  - `choose_batch_size` logic
- Integration:
  - index a small fixture
  - query returns results

## Order to Implement
1. `embed_batch`
2. `choose_batch_size`
3. `build_entries_for_file`
4. parallel indexing
5. update logic
6. tests
