# RustIntellivector

This is a sample document for testing the ingestion pipeline.
It covers vector similarity search implemented in Rust using memory-mapped files for efficient I/O.

## Motivation

Traditional in-memory approaches load entire datasets upfront. Memory-mapped files let the OS page in only the regions actually accessed, which is ideal for large corpora.

## Chunking Strategy

Documents are split into chunks of at most 512 characters, with natural paragraph boundaries (double newline) preferred as split points. This preserves semantic coherence within each chunk.
