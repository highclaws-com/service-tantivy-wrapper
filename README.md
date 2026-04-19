# Tantivy Wrapper Daemon

A full-text search service wrapper around [Tantivy](https://github.com/quickwit-oss/tantivy). It includes a custom Jieba tokenizer (`jieba-rs`) for robust Chinese text search support.

## Setup and Running
The daemon requires the `TANTIVY_INDEX_PATH` environment variable to be set, indicating where the index files should be stored.

```bash
# Set the index directory
export TANTIVY_INDEX_PATH=./index_data

# Run the server
cargo run
```

The server will start on `http://0.0.0.0:8080`.
