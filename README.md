# Rust-Intellivector

A local memory layer for LLMs, built in Rust. You feed it documents (and .rs files), ask it a question, and it returns the most relevant chunks of text (or code) entirely on your machine, no API calls required.

Under the hood it splits documents into chunks, converts them into vector embeddings using a local sentence transformer model, and finds the closest matches to your query via cosine similarity search.
