use indicatif::ProgressStyle;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

const DEFAULT_PROGRESS_TEMPLATE: &str =
    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {count}/{total} ({eta})";

/// Returns default progress style.
fn pb_style() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template(DEFAULT_PROGRESS_TEMPLATE)
        .progress_chars("=> ")
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct BlockDocument {
    pub terms: Vec<u16>,
    pub docs_impacts: Vec<Vec<(u8, u8)>>,
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct BlockForwardIndex {
    pub data: Vec<Vec<(u16, Vec<(u8, u8)>)>>,
    pub block_size: usize,
}

#[derive(Default, Serialize, Deserialize, Clone)]
pub struct ForwardIndex {
    data: Vec<Vec<(u32, u32)>>,
}
pub struct ForwardIndexBuilder {
    forward_index: ForwardIndex,
}
// Implement IntoIterator for a reference to PostingList.
impl<'a> IntoIterator for &'a ForwardIndex {
    type Item = &'a Vec<(u32, u32)>;
    type IntoIter = std::slice::Iter<'a, Vec<(u32, u32)>>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.iter()
    }
}

impl ForwardIndexBuilder {
    pub fn new(num_documents: usize) -> ForwardIndexBuilder {
        Self {
            forward_index: ForwardIndex {
                data: vec![Vec::new(); num_documents],
            },
        }
    }
    pub fn insert_posting_list(&mut self, term_id: u32, posting_list: &Vec<(u32, u32)>) {
        for (doc_id, score) in posting_list {
            self.forward_index.data[*doc_id as usize].push((term_id as u32, *score));
        }
    }
    pub fn insert_document(&mut self, vector: Vec<(u32, u32)>) {
        self.forward_index.data.push(vector);
    }
    pub fn build(&mut self) -> ForwardIndex {
        for doc in &mut self.forward_index.data {
            doc.sort_by_key(|d| d.0);
        }

        std::mem::take(&mut self.forward_index)
    }
}

pub fn fwd2bfwd(fwd: &ForwardIndex, block_size: usize) -> BlockForwardIndex {
    // Step 1: Group documents into blocks
    let blocks = fwd.data.par_chunks(block_size);
    let progress = indicatif::ProgressBar::new(blocks.len() as u64);
    progress.set_style(pb_style());
    progress.set_draw_delta((blocks.len() / 100) as u64);

    BlockForwardIndex {block_size: block_size, data:


    // Step 2: For each block, aggregate term-score pairs
    blocks
        .map(|block| {

            let mut term_pairs: Vec<(u32, u32, u32)> = block.iter().enumerate().flat_map(|(idx, doc)| {
                doc.iter().map(move|(term, score)| (*term, idx as u32, *score))
            }).collect();
            // Sort by term to aggregate them in the next step
            term_pairs.sort_by_key(|pair| pair.0);

            // Aggregate term-score pairs
            let mut aggregated: Vec<(u16, Vec<(u8, u8)>)> = Vec::new();
            let mut current_term = None;
            let mut current_scores = Vec::new();
            for (term,doc_id, score) in term_pairs {
                match current_term {
                    Some(t) if t == term => current_scores.push((
                        doc_id as u8,
                        score as u8,
                    )),
                    _ => {
                        if let Some(t) = current_term {
                            aggregated.push((t as u16, current_scores.clone()));
                            current_scores.clear();
                        }
                        current_term = Some(term);
                        current_scores.push((doc_id as u8,score as u8));
                    }
                }
            }
            if let Some(t) = current_term {
                aggregated.push((t as u16, current_scores));
            }
            progress.inc(1);

            aggregated
        })
        .collect()
    }
}

#[inline]
pub fn block_score(
    query: &Vec<(u16, u8)>,
    document: &[(u16, Vec<(u8, u8)>)],
    bsize: usize,
) -> Vec<u16> {
    let mut doc_score = vec![0; bsize];

    unsafe {
        let mut term_ptr = document.as_ptr();
        let end = term_ptr.wrapping_offset(document.len() as isize);
        for &(coordinate, value) in query {
            while term_ptr != end && (*term_ptr).0 < coordinate {
                term_ptr = term_ptr.add(1);
            }
            if term_ptr == end {
                break;
            }
            if (*term_ptr).0 == coordinate {
                let mut inner_ptr = (*term_ptr).1.as_ptr();
                let end_inner_ptr = inner_ptr.wrapping_offset((*term_ptr).1.len() as isize);
                while inner_ptr != end_inner_ptr {
                    doc_score[(*inner_ptr).0 as usize] += (value as u16) * ((*inner_ptr).1 as u16);
                    inner_ptr = inner_ptr.add(1);
                }
            }
        }
    }

    doc_score
}
