use jieba_rs::Jieba;
use tantivy::tokenizer::{Token, TokenStream, Tokenizer};
use std::sync::Arc;

// Set max chunk size in characters
const CHUNK_SIZE_CHARS: usize = 4096;

#[derive(Clone)]
pub struct CustomJiebaTokenizer {
    jieba: Arc<Jieba>,
}

impl CustomJiebaTokenizer {
    pub fn new(jieba: Jieba) -> Self {
        Self {
            jieba: Arc::new(jieba),
        }
    }
}

impl Tokenizer for CustomJiebaTokenizer {
    type TokenStream<'a> = CustomJiebaTokenStream<'a>;

    fn token_stream<'a>(&mut self, text: &'a str) -> Self::TokenStream<'a> {
        let mut stream = CustomJiebaTokenStream {
            jieba: self.jieba.clone(),
            text,
            current_byte_idx: 0,
            current_char_idx: 0,

            current_tokens: Vec::new(),
            token_index: 0,

            token: Token::default(),
            global_position_offset: 0,
        };

        // Prime the stream
        stream.load_next_chunk();
        stream
    }
}

pub struct CustomJiebaTokenStream<'a> {
    jieba: Arc<Jieba>,
    text: &'a str,

    current_byte_idx: usize,
    current_char_idx: usize,

    // The tokens for the current chunk
    current_tokens: Vec<jieba_rs::Token<'a>>,
    // Which token in the current chunk we are looking at
    token_index: usize,

    token: Token,
    // The character offset of the current chunk relative to the start of the full text
    global_position_offset: usize,
}

impl<'a> CustomJiebaTokenStream<'a> {
    fn load_next_chunk(&mut self) -> bool {
        if self.current_byte_idx >= self.text.len() {
            return false;
        }

        let start_byte = self.current_byte_idx;
        let mut end_byte = start_byte;
        let mut char_count = 0;

        for (idx, c) in self.text[start_byte..].char_indices() {
            if char_count >= CHUNK_SIZE_CHARS { break; }
            end_byte = start_byte + idx + c.len_utf8();
            char_count += 1;
        }

        let mut actual_char_count = char_count;

        // Try to break on a natural boundary if we aren't at the end of the text
        if end_byte < self.text.len() {
            let mut chars_looked_back = 0;
            // Look back up to 100 characters to find punctuation or whitespace
            for (idx, c) in self.text[start_byte..end_byte].char_indices().rev() {
                if chars_looked_back >= 100 { break; }
                if c.is_whitespace() || c.is_ascii_punctuation() || "，。！？；：、".contains(c) {
                    end_byte = start_byte + idx + c.len_utf8();
                    actual_char_count -= chars_looked_back;
                    break;
                }
                chars_looked_back += 1;
            }
            // If no break found, actual_char_count remains char_count (hard break at CHUNK_SIZE_CHARS)
        }

        let chunk = &self.text[start_byte..end_byte];

        self.global_position_offset = self.current_char_idx;
        self.current_tokens = self.jieba.tokenize(chunk, jieba_rs::TokenizeMode::Search, true);

        let preview_tokens: Vec<_> = self.current_tokens.iter().take(10).map(|t| t.word).collect();
        let more = if self.current_tokens.len() > 10 { " ..." } else { "" };
        println!("index={}: tokenizing chunk: {:?}{}", self.global_position_offset, preview_tokens, more);

        self.current_tokens.sort_by_key(|t| (t.start, t.end));
        self.token_index = 0;

        self.current_byte_idx = end_byte;
        self.current_char_idx += actual_char_count;

        !self.current_tokens.is_empty()
    }
}

impl<'a> TokenStream for CustomJiebaTokenStream<'a> {
    fn advance(&mut self) -> bool {
        loop {
            // If we've exhausted current chunk's tokens, load next chunk
            if self.token_index >= self.current_tokens.len() {
                if !self.load_next_chunk() {
                    return false;
                }
                continue; // Process newly loaded tokens
            }

            let t = &self.current_tokens[self.token_index];
            let word = t.word.trim();
            self.token_index += 1;

            // Skip whitespace tokens
            if word.is_empty() {
                continue;
            }

            self.token.text.clear();
            self.token.text.push_str(word);

            // Calculate absolute byte offsets in the original text
            self.token.offset_from = word.as_ptr() as usize - self.text.as_ptr() as usize;
            self.token.offset_to = self.token.offset_from + word.len();

            // Absolute character offset for position tracking
            self.token.position = self.global_position_offset + t.start;
            self.token.position_length = t.end - t.start;

            return true;
        }
    }

    fn token(&self) -> &Token {
        &self.token
    }

    fn token_mut(&mut self) -> &mut Token {
        &mut self.token
    }
}
