pub mod cursor;
pub mod live_block;
pub mod topk_heap;

use crate::index::inverted_index::Index;
use crate::index::posting_list::PostingListIterator;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

pub const MAX_TERM_WEIGHT: usize = 32;

pub fn cursors_from_queries<P: Into<PathBuf>>(
    queries_file: P,
    index: &Index,
) -> (Vec<String>, Vec<Vec<PostingListIterator>>) {
    let mut queries = Vec::new();
    let mut q_ids = Vec::new();

    let file = File::open(queries_file.into()).expect("Error opening file");
    let reader = BufReader::new(file);

    for (query_line, line) in reader.lines().enumerate() {
        let line = line.expect("Error reading line from file");

        // Split the line by ':' to separate the identifier and values
        let parts: Vec<&str> = line.splitn(2, ':').collect();

        if parts.len() == 2 {
            // Parse the query ID
            let q_id = parts[0].trim().to_string();
            q_ids.push(q_id);

            queries.push(cursors_from_query_text(parts[1], index));
        } else {
            panic!("Invalid line format in file at line {}", query_line);
        }
    }
    (q_ids, queries)
}

pub fn cursors_from_query_text<'a>(query: &str, index: &'a Index) -> Vec<PostingListIterator<'a>> {
    // Parse the values and create PostingList
    let values: Vec<&str> = query.split_whitespace().collect();
    let mut token_freqs: HashMap<String, u32> = HashMap::new();
    for t in values {
        *token_freqs.entry(t.to_string()).or_insert(0) += 1;
    }
    let max_tok_weight = token_freqs.iter().map(|p| *p.1).max().unwrap();
    if max_tok_weight > MAX_TERM_WEIGHT as u32 {
        let scale: f32 = MAX_TERM_WEIGHT as f32 / max_tok_weight as f32;
        for value in token_freqs.values_mut() {
            *value = (*value as f32 * scale).ceil() as u32;
        }
    }
    let cursors: Vec<PostingListIterator> = token_freqs
        .iter()
        .flat_map(|(token, freq)| index.get_cursor(token, *freq))
        .collect();
    cursors
}
