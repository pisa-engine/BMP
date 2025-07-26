use indicatif::ProgressStyle;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

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
    // Always use SIMD when available for maximum performance
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            unsafe { block_score_avx2(query, document, bsize) }
        } else if is_x86_feature_detected!("sse4.1") {
            unsafe { block_score_sse41(query, document, bsize) }
        } else {
            block_score_scalar(query, document, bsize)
        }
    }
    
    #[cfg(target_arch = "aarch64")]
    {
        unsafe { block_score_neon(query, document, bsize) }
    }
    
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        block_score_scalar(query, document, bsize)
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn block_score_avx2(
    query: &Vec<(u16, u8)>,
    document: &[(u16, Vec<(u8, u8)>)],
    bsize: usize,
) -> Vec<u16> {
    let mut doc_score = vec![0u16; bsize];
    
    // Process in chunks of 16 u16 values for maximum AVX2 utilization
    let simd_chunks = bsize & !15;
    
    // Use raw pointers for fastest access
    let mut term_ptr = document.as_ptr();
    let end_ptr = term_ptr.add(document.len());
    
    for &(term_id, weight) in query {
        // Optimized term search using pointer arithmetic
        while term_ptr < end_ptr && (*term_ptr).0 < term_id {
            term_ptr = term_ptr.add(1);
        }
        
        if term_ptr >= end_ptr {
            break;
        }
        
        if (*term_ptr).0 == term_id {
            let impact_list = &(*term_ptr).1;
            
            if impact_list.is_empty() {
                continue;
            }
            
            // For sparse patterns: accumulate in temp array then vectorize
            if impact_list.len() < bsize / 4 {
                // Sparse case: direct updates with SIMD when possible
                let weight_u16 = weight as u16;
                
                for &(doc_id, impact) in impact_list {
                    let idx = doc_id as usize;
                    if idx < bsize {
                        doc_score[idx] = doc_score[idx].saturating_add(weight_u16 * (impact as u16));
                    }
                }
            } else {
                // Dense case: use full SIMD pipeline
                let mut temp_scores = vec![0u16; bsize];
                
                // Accumulate impacts
                for &(doc_id, impact) in impact_list {
                    temp_scores[doc_id as usize] += impact as u16;
                }
                
                // Vectorized multiplication and accumulation
                let weight_vec = _mm256_set1_epi16(weight as i16);
                
                // Process 16 elements at a time
                for chunk in (0..simd_chunks).step_by(16) {
                    // Load impacts
                    let impacts_lo = _mm256_loadu_si256(temp_scores.as_ptr().add(chunk) as *const __m256i);
                    let impacts_hi = _mm256_loadu_si256(temp_scores.as_ptr().add(chunk + 8) as *const __m256i);
                    
                    // Multiply by weight
                    let weighted_lo = _mm256_mullo_epi16(impacts_lo, weight_vec);
                    let weighted_hi = _mm256_mullo_epi16(impacts_hi, weight_vec);
                    
                    // Load existing scores
                    let existing_lo = _mm256_loadu_si256(doc_score.as_ptr().add(chunk) as *const __m256i);
                    let existing_hi = _mm256_loadu_si256(doc_score.as_ptr().add(chunk + 8) as *const __m256i);
                    
                    // Add with saturation
                    let result_lo = _mm256_adds_epu16(existing_lo, weighted_lo);
                    let result_hi = _mm256_adds_epu16(existing_hi, weighted_hi);
                    
                    // Store results
                    _mm256_storeu_si256(doc_score.as_mut_ptr().add(chunk) as *mut __m256i, result_lo);
                    _mm256_storeu_si256(doc_score.as_mut_ptr().add(chunk + 8) as *mut __m256i, result_hi);
                }
                
                // Handle remaining elements
                let weight_u16 = weight as u16;
                for i in simd_chunks..bsize {
                    if temp_scores[i] > 0 {
                        doc_score[i] = doc_score[i].saturating_add(weight_u16 * temp_scores[i]);
                    }
                }
            }
        }
    }
    
    doc_score
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse4.1")]
#[inline]
unsafe fn block_score_sse41(
    query: &Vec<(u16, u8)>,
    document: &[(u16, Vec<(u8, u8)>)],
    bsize: usize,
) -> Vec<u16> {
    let mut doc_score = vec![0u16; bsize];
    
    // Process in chunks of 8 u16 values with SSE
    let simd_chunks = bsize & !7;
    
    let mut term_ptr = document.as_ptr();
    let end_ptr = term_ptr.add(document.len());
    
    for &(term_id, weight) in query {
        while term_ptr < end_ptr && (*term_ptr).0 < term_id {
            term_ptr = term_ptr.add(1);
        }
        
        if term_ptr >= end_ptr {
            break;
        }
        
        if (*term_ptr).0 == term_id {
            let impact_list = &(*term_ptr).1;
            
            if impact_list.len() < bsize / 4 {
                let weight_u16 = weight as u16;
                for &(doc_id, impact) in impact_list {
                    let idx = doc_id as usize;
                    if idx < bsize {
                        doc_score[idx] = doc_score[idx].saturating_add(weight_u16 * (impact as u16));
                    }
                }
            } else {
                let mut temp_scores = vec![0u16; bsize];
                
                for &(doc_id, impact) in impact_list {
                    temp_scores[doc_id as usize] += impact as u16;
                }
                
                let weight_vec = _mm_set1_epi16(weight as i16);
                
                for chunk in (0..simd_chunks).step_by(8) {
                    let impacts = _mm_loadu_si128(temp_scores.as_ptr().add(chunk) as *const __m128i);
                    let weighted = _mm_mullo_epi16(impacts, weight_vec);
                    let existing = _mm_loadu_si128(doc_score.as_ptr().add(chunk) as *const __m128i);
                    let result = _mm_adds_epu16(existing, weighted);
                    _mm_storeu_si128(doc_score.as_mut_ptr().add(chunk) as *mut __m128i, result);
                }
                
                let weight_u16 = weight as u16;
                for i in simd_chunks..bsize {
                    if temp_scores[i] > 0 {
                        doc_score[i] = doc_score[i].saturating_add(weight_u16 * temp_scores[i]);
                    }
                }
            }
        }
    }
    
    doc_score
}

#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
#[inline]
unsafe fn block_score_neon(
    query: &Vec<(u16, u8)>,
    document: &[(u16, Vec<(u8, u8)>)],
    bsize: usize,
) -> Vec<u16> {
    let mut doc_score = vec![0u16; bsize];
    
    // NEON processes 8 u16s efficiently, but we can interleave for better throughput
    let simd_chunks = bsize & !15; // Process 16 elements using two NEON registers
    
    let mut term_ptr = document.as_ptr();
    let end_ptr = term_ptr.add(document.len());
    
    for &(term_id, weight) in query {
        // Optimized term search
        while term_ptr < end_ptr && (*term_ptr).0 < term_id {
            term_ptr = term_ptr.add(1);
        }
        
        if term_ptr >= end_ptr {
            break;
        }
        
        if (*term_ptr).0 == term_id {
            let impact_list = &(*term_ptr).1;
            
            if impact_list.is_empty() {
                continue;
            }
            
            let weight_u16 = weight as u16;
            
            // For very sparse impact lists, direct scatter is more efficient
            if impact_list.len() <= 8 {
                for &(doc_id, impact) in impact_list {
                    let idx = doc_id as usize;
                    if idx < bsize {
                        doc_score[idx] = doc_score[idx].saturating_add(weight_u16 * (impact as u16));
                    }
                }
            }
            // For medium sparsity, use NEON gather-scatter pattern
            else if impact_list.len() < bsize / 8 {
                let weight_vec = vdupq_n_u16(weight_u16);
                
                // Process impacts in batches of 8 for optimal NEON utilization
                for impact_chunk in impact_list.chunks(8) {
                    let mut indices = [0usize; 8];
                    let mut impacts = [0u16; 8];
                    let mut count = 0;
                    
                    for &(doc_id, impact) in impact_chunk {
                        let idx = doc_id as usize;
                        if idx < bsize {
                            indices[count] = idx;
                            impacts[count] = impact as u16;
                            count += 1;
                        }
                    }
                    
                    if count > 0 {
                        // Load impacts into NEON register
                        let impact_vec = vld1q_u16(impacts.as_ptr());
                        
                        // Multiply-accumulate: impact * weight + existing_score
                        // This is more efficient than separate multiply + add
                        for i in 0..count {
                            let idx = indices[i];
                            let current = doc_score[idx];
                            let increment = impacts[i] * weight_u16;
                            doc_score[idx] = current.saturating_add(increment);
                        }
                    }
                }
            }
            // For dense cases, use full vectorization with interleaved processing
            else {
                let mut temp_scores = vec![0u16; bsize];
                
                // Accumulate impacts efficiently
                for &(doc_id, impact) in impact_list {
                    let idx = doc_id as usize;
                    if idx < bsize {
                        temp_scores[idx] += impact as u16;
                    }
                }
                
                let weight_vec = vdupq_n_u16(weight_u16);
                
                // Process 16 elements at a time using two interleaved NEON registers
                // This maximizes memory bandwidth and instruction throughput
                for chunk in (0..simd_chunks).step_by(16) {
                    // Load two consecutive 8-element vectors
                    let impacts_0 = vld1q_u16(temp_scores.as_ptr().add(chunk));
                    let impacts_1 = vld1q_u16(temp_scores.as_ptr().add(chunk + 8));
                    
                    // Load existing scores
                    let existing_0 = vld1q_u16(doc_score.as_ptr().add(chunk));
                    let existing_1 = vld1q_u16(doc_score.as_ptr().add(chunk + 8));
                    
                    // Use multiply-accumulate instruction for optimal performance
                    // vmlaq_u16 computes: existing + (impacts * weight)
                    let result_0 = vmlaq_u16(existing_0, impacts_0, weight_vec);
                    let result_1 = vmlaq_u16(existing_1, impacts_1, weight_vec);
                    
                    // Store results back
                    vst1q_u16(doc_score.as_mut_ptr().add(chunk), result_0);
                    vst1q_u16(doc_score.as_mut_ptr().add(chunk + 8), result_1);
                }
                
                // Handle remaining elements with scalar code
                for i in simd_chunks..bsize {
                    if temp_scores[i] > 0 {
                        doc_score[i] = doc_score[i].saturating_add(weight_u16 * temp_scores[i]);
                    }
                }
            }
        }
    }
    
    doc_score
}

#[inline]
fn block_score_scalar(
    query: &Vec<(u16, u8)>,
    document: &[(u16, Vec<(u8, u8)>)],
    bsize: usize,
) -> Vec<u16> {
    let mut doc_score = vec![0u16; bsize];

    unsafe {
        let mut term_ptr = document.as_ptr();
        let end = term_ptr.add(document.len());
        
        for &(coordinate, value) in query {
            while term_ptr < end && (*term_ptr).0 < coordinate {
                term_ptr = term_ptr.add(1);
            }
            if term_ptr >= end {
                break;
            }
            if (*term_ptr).0 == coordinate {
                let impact_list = &(*term_ptr).1;
                let weight = value as u16;
                
                for &(doc_id, impact) in impact_list {
                    let idx = doc_id as usize;
                    if idx < bsize {
                        doc_score[idx] = doc_score[idx].saturating_add(weight * (impact as u16));
                    }
                }
            }
        }
    }

    doc_score
}
