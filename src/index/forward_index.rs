use indicatif::ProgressStyle;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use wide::u16x8;
use sprs::{CsMat, TriMat};
use std::collections::HashMap;

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
    // Use original implementation - it's actually faster for most cases
    block_score_original(query, document, bsize)
}

// Original implementation kept for compatibility
#[inline]
pub fn block_score_original(
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

#[inline]
pub fn block_score_sparse(
    query_sparse: &sprs::CsVec<u16>,
    document_matrix: &CsMat<u16>,
) -> Vec<u16> {
    // Sparse matrix-vector multiplication
    let result = document_matrix * query_sparse;
    result.data().to_vec()
}

// SIMD version for when it's actually beneficial (very specific cases)
#[inline]
pub fn block_score_simd_opt(
    query: &Vec<(u16, u8)>,
    document: &[(u16, Vec<(u8, u8)>)],
    bsize: usize,
) -> Vec<u16> {
    // Only use SIMD for large blocks where overhead is justified
    if bsize >= 256 && query.len() > 10 {
        let mut doc_score = vec![0u16; bsize];
        
        for &(query_term, query_weight) in query {
            // Binary search for term in document
            if let Ok(term_idx) = document.binary_search_by_key(&query_term, |&(term, _)| term) {
                let impact_list = &document[term_idx].1;
                
                // Process in chunks of 8 for SIMD
                let chunks = bsize / 8;
                for chunk_start in (0..chunks * 8).step_by(8) {
                    let mut scores_array = [0u16; 8];
                    
                    for &(doc_offset, impact) in impact_list {
                        let doc_idx = doc_offset as usize;
                        if doc_idx >= chunk_start && doc_idx < chunk_start + 8 {
                            let lane = doc_idx - chunk_start;
                            scores_array[lane] += (query_weight as u16) * (impact as u16);
                        }
                    }
                    
                    // Simple addition without complex SIMD operations
                    for (i, &score_inc) in scores_array.iter().enumerate() {
                        doc_score[chunk_start + i] += score_inc;
                    }
                }
                
                // Handle remainder
                for &(doc_offset, impact) in impact_list {
                    let doc_idx = doc_offset as usize;
                    if doc_idx >= chunks * 8 {
                        doc_score[doc_idx] += (query_weight as u16) * (impact as u16);
                    }
                }
            }
        }
        doc_score
    } else {
        // Fall back to original for small blocks
        block_score_original(query, document, bsize)
    }
}

// Sparse matrix representation for better memory efficiency (no serialization)
pub struct SparseBlockForwardIndex {
    pub sparse_data: Vec<CsMat<u16>>, // Compressed sparse row matrices
    pub block_size: usize,
    pub num_terms: usize,
}

impl SparseBlockForwardIndex {
    pub fn from_block_forward_index(bfi: &BlockForwardIndex) -> Self {
        let mut sparse_data = Vec::new();
        let mut max_term_id = 0;
        
        for block in &bfi.data {
            // Find dimensions
            let mut doc_ids = Vec::new();
            let mut term_ids = Vec::new();
            let mut scores = Vec::new();
            let mut max_doc_id = 0;
            
            for (term_id, doc_score_pairs) in block {
                max_term_id = max_term_id.max(*term_id);
                for &(doc_id, score) in doc_score_pairs {
                    doc_ids.push(doc_id as usize);
                    term_ids.push(*term_id as usize);
                    scores.push(score as u16);
                    max_doc_id = max_doc_id.max(doc_id as usize);
                }
            }
            
            // Create sparse matrix (documents x terms)
            let mut triplet_mat = TriMat::new((max_doc_id + 1, max_term_id as usize + 1));
            for ((doc_id, term_id), score) in doc_ids.iter().zip(term_ids.iter()).zip(scores.iter()) {
                triplet_mat.add_triplet(*doc_id, *term_id, *score);
            }
            
            sparse_data.push(triplet_mat.to_csr());
        }
        
        SparseBlockForwardIndex {
            sparse_data,
            block_size: bfi.block_size,
            num_terms: max_term_id as usize + 1,
        }
    }
}

// Optimized sparse query representation
pub struct SparseQuery {
    pub term_ids: Vec<usize>,
    pub weights: Vec<u16>,
    pub sparse_vec: sprs::CsVec<u16>,
}

impl SparseQuery {
    pub fn from_query_vec(query_vec: &[(u16, u8)], num_terms: usize) -> Self {
        let mut term_ids = Vec::new();
        let mut weights = Vec::new();
        
        for &(term_id, weight) in query_vec {
            term_ids.push(term_id as usize);
            weights.push(weight as u16);
        }
        
        let sparse_vec = sprs::CsVec::new(num_terms, term_ids.clone(), weights.clone());
        
        SparseQuery {
            term_ids,
            weights,
            sparse_vec,
        }
    }
}
