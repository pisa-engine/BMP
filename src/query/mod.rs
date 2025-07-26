pub mod cursor;
pub mod live_block;
pub mod topk_heap;

use crate::index::inverted_index::Index;
use crate::index::posting_list::PostingListIterator;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use serde_json;

pub const MAX_TERM_WEIGHT: usize = 32;

pub fn cursors_from_queries<P: Into<PathBuf>>(
    queries_file: P,
    index: &Index,
) -> (Vec<String>, Vec<Vec<PostingListIterator>>) {
    let mut queries = Vec::new();
    let mut q_ids = Vec::new();

    let queries_path = queries_file.into();
    let file = File::open(&queries_path).expect("Error opening file");
    let reader = BufReader::new(file);

    // Try to parse first line to detect format
    let first_line = reader.lines().next().expect("File is empty").expect("Error reading line");
    let is_json = first_line.trim_start().starts_with('{');
    eprintln!("is_json: {}", is_json);
    // Reset reader
    let file = File::open(&queries_path).expect("Error opening file");
    let reader = BufReader::new(file);

    for (query_line, line) in reader.lines().enumerate() {
        let line = line.expect("Error reading line from file");

        if is_json {
            // Parse JSON format
            let v: serde_json::Value = serde_json::from_str(&line)
                .unwrap_or_else(|_| panic!("Invalid JSON at line {}", query_line));

            let id = v["id"].as_u64()
                .unwrap_or_else(|| panic!("Missing or invalid id at line {}", query_line))
                .to_string();
            q_ids.push(id);

            let vector = v["vector"].as_object()
                .unwrap_or_else(|| panic!("Missing or invalid vector at line {}", query_line));

            let mut token_freqs: HashMap<String, u32> = HashMap::new();
            for (token, score) in vector {
                let score = (score.as_f64().unwrap() * 10.0).ceil() as u32;
                token_freqs.insert(token.to_string(), score);
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
            queries.push(cursors);

        } else {
            // Parse original format (id:tokens)
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() == 2 {
                q_ids.push(parts[0].trim().to_string());
                queries.push(cursors_from_query_text(parts[1], index));
            } else {
                panic!("Invalid line format at line {}", query_line);
            }
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
