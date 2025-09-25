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
    pub data: Vec<Vec<(u16, (Vec<u8>, Vec<u8>))>>,
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
            let mut aggregated: Vec<(u16, (Vec<u8>, Vec<u8>))> = Vec::new();
            let mut current_term = None;
            let mut current_doc_ids = Vec::new();
            let mut current_scores = Vec::new();
            for (term,doc_id, score) in term_pairs {
                match current_term {
                    Some(t) if t == term => {
                        current_doc_ids.push(doc_id as u8);
                        current_scores.push(score as u8);
                    },
                    _ => {
                        if let Some(t) = current_term {
                            aggregated.push((t as u16, (current_doc_ids.clone(), current_scores.clone())));
                            current_doc_ids.clear();
                            current_scores.clear();
                        }
                        current_term = Some(term);
                        current_doc_ids.push(doc_id as u8);
                        current_scores.push(score as u8);
                    }
                }
            }
            if let Some(t) = current_term {
                aggregated.push((t as u16, (current_doc_ids, current_scores)));
            }
            progress.inc(1);

            aggregated
        })
        .collect()
    }
}

#[cfg(all(target_feature = "avx512f", target_feature = "avx512bw"))]
#[inline]
pub fn block_score(
    query: &[(u16, u8)],
    document: &[(u16, (Vec<u8>, Vec<u8>))],
    bsize: usize,
) -> Vec<u16> {
    use std::arch::x86_64::*;
    assert!(bsize == 16);

    let mut doc_scores = vec![0i32; bsize];

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
                let doc_ids = &(*term_ptr).1.0;
                let len = doc_ids.len();
                
                // Load doc_ids and scores as u8 vectors
                let docs = _mm_loadu_si128((*term_ptr).1.0.as_ptr() as *const __m128i);
                let docs_i32 = _mm512_cvtepu8_epi32(docs);

                // packed u8 scores in the same order as docs
                let scores_v = _mm_loadu_si128((*term_ptr).1.1.as_ptr() as *const __m128i);

                // packed u8 scores to packed i32
                let scores_i32 = _mm512_cvtepu8_epi32(scores_v);

                // Broadcast the query value
                let query_value = _mm512_set1_epi32(value as i32);

                // Multiply the scores by the query value
                let term_scores = _mm512_mullo_epi32(scores_i32, query_value);

                // Gather previous doc_scores at doc_ids
                let prev_scores_at_docs = _mm512_i32gather_epi32(docs_i32, doc_scores.as_ptr() as *const i32, 4);

                // Add the term scores to the previous doc scores
                let new_scores = _mm512_add_epi32(prev_scores_at_docs, term_scores);

                // Scatter the new scores back to the doc_scores at corresponding positions
                // let scores_mask = ((1u16 << len) - 1) as __mmask16;
                // _mm512_mask_i32scatter_epi32(doc_scores.as_mut_ptr() as *mut i32, scores_mask, docs_i32, new_scores, 4);
            }
        }
    }

    doc_scores
        .into_iter()
        .map(|x| x as u16)
        .collect()
}

#[inline]
pub fn block_score_scalar(
    query: &[(u16, u8)],
    document: &[(u16, (Vec<u8>, Vec<u8>))],
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
                let (doc_ids, scores) = &(*term_ptr).1;
                let doc_ids_ptr = doc_ids.as_ptr();
                let scores_ptr = scores.as_ptr();
                let len = doc_ids.len();
                
                for i in 0..len {
                    let doc_id = *doc_ids_ptr.add(i) as usize;
                    let score = *scores_ptr.add(i) as u16;
                    doc_score[doc_id] += (value as u16) * score;
                }
            }
        }
    }

    doc_score
}

#[cfg(not(all(target_feature = "avx512f", target_feature = "avx512bw")))]
#[inline]
pub fn block_score(
    query: &[(u16, u8)],
    document: &[(u16, (Vec<u8>, Vec<u8>))],
    bsize: usize,
) -> Vec<u16> {
    block_score_scalar(query, document, bsize)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_score_basic() {
        // Query: term 1 with weight 2, term 2 with weight 3
        let query = vec![(1u16, 2u8), (2u16, 3u8)];
        // Document: term 1 appears in doc 0 (score 5), term 2 in doc 1 (score 7)
        let document = vec![
            (1u16, (vec![0u8], vec![5u8])),
            (2u16, (vec![1u8], vec![7u8])),
        ];
        let bsize = 16;
        let result = block_score(&query, &document, bsize);
        // doc 0: 2*5 = 10, doc 1: 3*7 = 21, doc 2..15: 0
        let mut expected = vec![0u16; 16];
        expected[0] = 10;
        expected[1] = 21;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_block_score_multiple_postings() {
        // Query: term 1 with weight 1
        let query = vec![(1u16, 1u8)];
        // Document: term 1 appears in doc 0 (score 2), doc 1 (score 3), doc 2 (score 4)
        let document = vec![
            (1u16, (vec![0u8, 1u8, 2u8], vec![2u8, 3u8, 4u8])),
        ];
        let bsize = 16;
        let result = block_score(&query, &document, bsize);
        let mut expected = vec![0u16; 16];
        expected[0] = 2;
        expected[1] = 3;
        expected[2] = 4;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_block_score_empty_query() {
        let query = vec![];
        let document = vec![
            (1u16, (vec![0u8], vec![5u8])),
        ];
        let bsize = 16;
        let result = block_score(&query, &document, bsize);
        assert_eq!(result, vec![0u16; 16]);
    }

    #[test]
    fn test_block_score_empty_document() {
        let query = vec![(1u16, 2u8)];
        let document: Vec<(u16, (Vec<u8>, Vec<u8>))> = vec![];
        let bsize = 16;
        let result = block_score(&query, &document, bsize);
        assert_eq!(result, vec![0u16; 16]);
    }

    #[test]
    fn test_block_score_multiple_terms_and_docs() {
        // Query: term 1 (weight 2), term 3 (weight 4)
        let query = vec![(1u16, 2u8), (3u16, 4u8)];
        // Document: term 1 in doc 0 (score 1), doc 2 (score 2)
        //           term 2 in doc 1 (score 3)
        //           term 3 in doc 0 (score 2), doc 2 (score 1)
        let document = vec![
            (1u16, (vec![0u8, 2u8], vec![1u8, 2u8])),
            (2u16, (vec![1u8], vec![3u8])),
            (3u16, (vec![0u8, 2u8], vec![2u8, 1u8])),
        ];
        let bsize = 16;
        let result = block_score(&query, &document, bsize);
        // doc 0: 2*1 + 4*2 = 2 + 8 = 10
        // doc 1: 0
        // doc 2: 2*2 + 4*1 = 4 + 4 = 8
        let mut expected = vec![0u16; 16];
        expected[0] = 10;
        expected[2] = 8;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_block_score_unique_doc_ids() {
        // Query: term 1 (weight 2)
        let query = vec![(1u16, 2u8)];
        // Document: term 1 in doc 0 (score 3) and doc 1 (score 4)
        let document = vec![
            (1u16, (vec![0u8, 1u8], vec![3u8, 4u8])),
        ];
        let bsize = 16;
        let result = block_score(&query, &document, bsize);
        // doc 0: 2*3 = 6, doc 1: 2*4 = 8
        let mut expected = vec![0u16; 16];
        expected[0] = 6;
        expected[1] = 8;
        assert_eq!(result, expected);
    }

    #[test]
    fn test_block_score_bsize_larger_than_docs() {
        // Query: term 1 (weight 1)
        let query = vec![(1u16, 1u8)];
        // Document: term 1 in doc 0 (score 5)
        let document = vec![
            (1u16, (vec![0u8], vec![5u8])),
        ];
        let bsize = 16;
        let result = block_score(&query, &document, bsize);
        let mut expected = vec![0u16; 16];
        expected[0] = 5;
        assert_eq!(result, expected);
    }
}
