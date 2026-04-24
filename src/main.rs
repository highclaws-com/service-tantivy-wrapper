mod tokenizer;

use axum::{
    extract::{State, DefaultBodyLimit},
    routing::post,
    Json, Router,
};
use tokenizer::CustomJiebaTokenizer;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tantivy::collector::TopDocs;
use tantivy::schema::{IndexRecordOption, Schema, TextFieldIndexing, TextOptions, STORED, STRING, Value, FAST};
use tantivy::tokenizer::{Language, LowerCaser, Stemmer, TextAnalyzer};
use tantivy::{Index, IndexWriter};
use tantivy::directory::MmapDirectory;
use tantivy::TantivyDocument;

#[derive(Clone)]
struct AppState {
    index: Index,
    writer: Arc<tokio::sync::Mutex<IndexWriter>>,
}

#[derive(Deserialize)]
struct IndexRequest {
    hash: String,
    text: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    start_byte: u64,
    #[serde(default)]
    end_byte: u64,
    #[serde(default)]
    mtime: u64,
}

#[derive(Deserialize)]
struct IndexPayload {
    docs: Vec<IndexRequest>,
    commit: Option<bool>,
}

#[derive(Deserialize)]
struct DeleteRequest {
    hashes: Vec<String>,
    commit: Option<bool>,
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default)]
    tags: Option<Vec<String>>,
    limit: Option<usize>,
    snippet_length: Option<usize>,
    mtime_gte: Option<u64>,
    mtime_lte: Option<u64>,
}

#[derive(Serialize)]
struct SearchResponse {
    hash: String,
    score: f32,
    snippet: String,
    start_byte: u64,
    end_byte: u64,
    mtime: u64,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let mut schema_builder = Schema::builder();

    schema_builder.add_text_field("hash", STRING | STORED);

    let text_indexing = TextFieldIndexing::default()
        .set_tokenizer("custom_jieba")
        .set_index_option(IndexRecordOption::WithFreqsAndPositions);
    let text_options = TextOptions::default()
        .set_indexing_options(text_indexing)
        .set_stored();
    schema_builder.add_text_field("text", text_options);
    schema_builder.add_text_field("tags", STRING);
    schema_builder.add_u64_field("start_byte", STORED);
    schema_builder.add_u64_field("end_byte", STORED);
    schema_builder.add_u64_field("mtime", STORED | FAST);

    let schema = schema_builder.build();

    let index_dir = std::env::var("TANTIVY_INDEX_PATH").expect("TANTIVY_INDEX_PATH must be set");
    let index_path = std::path::Path::new(&index_dir);
    std::fs::create_dir_all(&index_path).unwrap();
    let mmap_directory = MmapDirectory::open(&index_path).unwrap();

    let index = match Index::open(mmap_directory.clone()) {
        Ok(idx) => idx,
        Err(_) => Index::create(mmap_directory, schema.clone(), tantivy::IndexSettings::default()).unwrap(),
    };

    let jieba = jieba_rs::Jieba::new();
    let custom_tokenizer = CustomJiebaTokenizer::new(jieba);
    let analyzer = TextAnalyzer::builder(custom_tokenizer)
        .filter(LowerCaser)
        .filter(Stemmer::new(Language::English))
        .build();
    index.tokenizers().register("custom_jieba", analyzer);

    // 128MB: To comfortably handle large documents (up to 64MB) plus index overhead
    let writer = index.writer(128_000_000).unwrap();
    let state = AppState {
        index,
        writer: Arc::new(tokio::sync::Mutex::new(writer)),
    };

    let app = Router::new()
        .route("/index_docs", post(index_docs))
        .route("/delete", post(delete_docs))
        .route("/search", post(search))
        // Set body limit to 128MB to accommodate payload up to 64MB + JSON overhead
        .layer(DefaultBodyLimit::max(128 * 1024 * 1024))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("Tantivy FTS daemon running on http://0.0.0.0:8080");
    axum::serve(listener, app).await.unwrap();
}

async fn index_docs(
    State(state): State<AppState>,
    Json(payload): Json<IndexPayload>,
) -> Json<&'static str> {
    let schema = state.index.schema();
    let hash_field = schema.get_field("hash").unwrap();
    let text_field = schema.get_field("text").unwrap();
    let start_byte_field = schema.get_field("start_byte").unwrap();
    let end_byte_field = schema.get_field("end_byte").unwrap();
    let mtime_field = schema.get_field("mtime").unwrap();
    let tags_field = schema.get_field("tags").unwrap();

    let mut writer = state.writer.lock().await;

    for req in payload.docs {
        let mut doc = TantivyDocument::default();
        doc.add_text(hash_field, &req.hash);
        doc.add_text(text_field, &req.text);
        doc.add_u64(start_byte_field, req.start_byte);
        doc.add_u64(end_byte_field, req.end_byte);
        doc.add_u64(mtime_field, req.mtime);
        for tag in req.tags {
            doc.add_text(tags_field, &tag);
        }
        writer.add_document(doc).unwrap();
    }

    if payload.commit.unwrap_or(true) {
        writer.commit().unwrap();
    }

    Json("ok")
}

async fn delete_docs(
    State(state): State<AppState>,
    Json(payload): Json<DeleteRequest>,
) -> Json<&'static str> {
    let schema = state.index.schema();
    let hash_field = schema.get_field("hash").unwrap();

    let mut writer = state.writer.lock().await;
    for hash in payload.hashes {
        let term = tantivy::Term::from_field_text(hash_field, &hash);
        writer.delete_term(term);
    }

    if payload.commit.unwrap_or(true) {
        writer.commit().unwrap();
    }

    Json("ok")
}

async fn search(State(state): State<AppState>, Json(query): Json<SearchQuery>) -> Json<Vec<SearchResponse>> {
    println!("--- SEARCH REQUEST ---");
    println!("q: {:?}, tags: {:?}, mtime_gte: {:?}, mtime_lte: {:?}", query.q, query.tags, query.mtime_gte, query.mtime_lte);

    let reader = state.index.reader().unwrap();
    let searcher = reader.searcher();

    let schema = state.index.schema();
    let hash_field = schema.get_field("hash").unwrap();
    let text_field = schema.get_field("text").unwrap();

    let mut analyzer = state.index.tokenizers().get("custom_jieba").unwrap();
    let mut token_stream = analyzer.token_stream(&query.q);

    let mut terms_with_offset = Vec::new();
    while let Some(token) = token_stream.next() {
        let term = tantivy::Term::from_field_text(text_field, &token.text);
        terms_with_offset.push((token.position, term));
    }

    let mut sub_queries: Vec<(tantivy::query::Occur, Box<dyn tantivy::query::Query>)> = Vec::new();

    if !terms_with_offset.is_empty() {
        let parsed_query: Box<dyn tantivy::query::Query> = if terms_with_offset.len() == 1 {
            Box::new(tantivy::query::TermQuery::new(terms_with_offset[0].1.clone(), IndexRecordOption::WithFreqsAndPositions))
        } else {
            Box::new(tantivy::query::PhraseQuery::new_with_offset_and_slop(terms_with_offset, 10))
        };
        sub_queries.push((tantivy::query::Occur::Must, parsed_query));
    }

    if let Some(tags) = &query.tags {
        if !tags.is_empty() {
            let tags_field = schema.get_field("tags").unwrap();
            for tag in tags {
                let tag_term = tantivy::Term::from_field_text(tags_field, tag);
                let tag_query = tantivy::query::TermQuery::new(tag_term, IndexRecordOption::Basic);
                sub_queries.push((tantivy::query::Occur::Must, Box::new(tag_query)));
            }
        }
    }

    if query.mtime_gte.is_some() || query.mtime_lte.is_some() {
        let start = query.mtime_gte.map(std::ops::Bound::Included).unwrap_or(std::ops::Bound::Unbounded);
        let end = query.mtime_lte.map(std::ops::Bound::Included).unwrap_or(std::ops::Bound::Unbounded);
        let range_query = Box::new(tantivy::query::RangeQuery::new_u64_bounds(
            "mtime".to_string(),
            start,
            end
        ));
        sub_queries.push((tantivy::query::Occur::Must, range_query));
    }

    let final_query: Box<dyn tantivy::query::Query> = if sub_queries.is_empty() {
        Box::new(tantivy::query::AllQuery)
    } else if sub_queries.len() == 1 {
        sub_queries.remove(0).1
    } else {
        Box::new(tantivy::query::BooleanQuery::new(sub_queries))
    };

    let limit = query.limit.unwrap_or(10);
    println!("Executing search with final_query: {:?}", final_query);
    let top_docs = searcher.search(&final_query, &TopDocs::with_limit(limit)).unwrap();
    println!("Found {} top docs", top_docs.len());

    let mut snippet_generator = tantivy::SnippetGenerator::create(&searcher, &*final_query, text_field).unwrap();
    let snippet_length = query.snippet_length.unwrap_or(150);
    snippet_generator.set_max_num_chars(snippet_length);

    let mut results = Vec::new();
    for (score, doc_address) in top_docs {
        let retrieved_doc = searcher.doc::<TantivyDocument>(doc_address).unwrap();
        let hash = retrieved_doc.get_first(hash_field).unwrap().as_str().unwrap().to_string();
        let start_byte = retrieved_doc.get_first(schema.get_field("start_byte").unwrap()).map(|v| v.as_u64().unwrap_or(0)).unwrap_or(0);
        let end_byte = retrieved_doc.get_first(schema.get_field("end_byte").unwrap()).map(|v| v.as_u64().unwrap_or(0)).unwrap_or(0);
        let mtime = retrieved_doc.get_first(schema.get_field("mtime").unwrap()).map(|v| v.as_u64().unwrap_or(0)).unwrap_or(0);

        let snippet = snippet_generator.snippet_from_doc(&retrieved_doc);

        results.push(SearchResponse {
            hash,
            score,
            snippet: snippet.to_html(),
            start_byte,
            end_byte,
            mtime,
        });
    }

    Json(results)
}

