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
                let impacts = &(*term_ptr).1;
                let mut inner_ptr = impacts.as_ptr();
                let end_inner_ptr = inner_ptr.wrapping_offset(impacts.len() as isize);
                
                // SIMD optimization for bulk processing
                #[cfg(target_arch = "x86_64")]
                {
                    if is_x86_feature_detected!("avx2") && impacts.len() >= 8 {
                        block_score_simd_x86_64(&mut doc_score, inner_ptr, end_inner_ptr, value);
                    } else {
                        block_score_scalar(&mut doc_score, inner_ptr, end_inner_ptr, value);
                    }
                }
                
                #[cfg(target_arch = "aarch64")]
                {
                    if std::arch::is_aarch64_feature_detected!("neon") && impacts.len() >= 8 {
                        block_score_simd_aarch64(&mut doc_score, inner_ptr, end_inner_ptr, value);
                    } else {
                        block_score_scalar(&mut doc_score, inner_ptr, end_inner_ptr, value);
                    }
                }
                
                #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
                {
                    block_score_scalar(&mut doc_score, inner_ptr, end_inner_ptr, value);
                }
            }
        }
    }

    doc_score
}

#[inline]
unsafe fn block_score_scalar(
    doc_score: &mut [u16],
    mut inner_ptr: *const (u8, u8),
    end_inner_ptr: *const (u8, u8),
    value: u8,
) {
    while inner_ptr != end_inner_ptr {
        doc_score[(*inner_ptr).0 as usize] += (value as u16) * ((*inner_ptr).1 as u16);
        inner_ptr = inner_ptr.add(1);
    }
}

#[cfg(target_arch = "x86_64")]
#[inline]
unsafe fn block_score_simd_x86_64(
    doc_score: &mut [u16],
    mut inner_ptr: *const (u8, u8),
    end_inner_ptr: *const (u8, u8),
    value: u8,
) {
    let value_vec = _mm256_set1_epi16(value as i16);
    
    while inner_ptr.wrapping_offset(8) <= end_inner_ptr {
        // Load 8 pairs of (offset, impact)
        let data = _mm256_loadu_si256(inner_ptr as *const __m256i);
        
        // Extract offsets (first 8 bytes)
        let offsets = _mm256_and_si256(data, _mm256_set1_epi16(0xFF));
        
        // Extract impacts (second 8 bytes)
        let impacts = _mm256_srli_epi16(data, 8);
        let impacts = _mm256_and_si256(impacts, _mm256_set1_epi16(0xFF));
        
        // Multiply impacts by value
        let scaled_impacts = _mm256_mullo_epi16(impacts, value_vec);
        
        // Process each element using unrolled loop
        let offset0 = _mm256_extract_epi16(offsets, 0) as usize;
        let impact0 = _mm256_extract_epi16(scaled_impacts, 0) as u16;
        if offset0 < doc_score.len() { doc_score[offset0] += impact0; }
        
        let offset1 = _mm256_extract_epi16(offsets, 1) as usize;
        let impact1 = _mm256_extract_epi16(scaled_impacts, 1) as u16;
        if offset1 < doc_score.len() { doc_score[offset1] += impact1; }
        
        let offset2 = _mm256_extract_epi16(offsets, 2) as usize;
        let impact2 = _mm256_extract_epi16(scaled_impacts, 2) as u16;
        if offset2 < doc_score.len() { doc_score[offset2] += impact2; }
        
        let offset3 = _mm256_extract_epi16(offsets, 3) as usize;
        let impact3 = _mm256_extract_epi16(scaled_impacts, 3) as u16;
        if offset3 < doc_score.len() { doc_score[offset3] += impact3; }
        
        let offset4 = _mm256_extract_epi16(offsets, 4) as usize;
        let impact4 = _mm256_extract_epi16(scaled_impacts, 4) as u16;
        if offset4 < doc_score.len() { doc_score[offset4] += impact4; }
        
        let offset5 = _mm256_extract_epi16(offsets, 5) as usize;
        let impact5 = _mm256_extract_epi16(scaled_impacts, 5) as u16;
        if offset5 < doc_score.len() { doc_score[offset5] += impact5; }
        
        let offset6 = _mm256_extract_epi16(offsets, 6) as usize;
        let impact6 = _mm256_extract_epi16(scaled_impacts, 6) as u16;
        if offset6 < doc_score.len() { doc_score[offset6] += impact6; }
        
        let offset7 = _mm256_extract_epi16(offsets, 7) as usize;
        let impact7 = _mm256_extract_epi16(scaled_impacts, 7) as u16;
        if offset7 < doc_score.len() { doc_score[offset7] += impact7; }
        
        inner_ptr = inner_ptr.add(8);
    }
    
    // Handle remaining elements
    while inner_ptr != end_inner_ptr {
        doc_score[(*inner_ptr).0 as usize] += (value as u16) * ((*inner_ptr).1 as u16);
        inner_ptr = inner_ptr.add(1);
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn block_score_simd_aarch64(
    doc_score: &mut [u16],
    mut inner_ptr: *const (u8, u8),
    end_inner_ptr: *const (u8, u8),
    value: u8,
) {
    let value_vec = vdupq_n_u16(value as u16);
    
    while inner_ptr.wrapping_offset(8) <= end_inner_ptr {
        // Load 8 pairs of (offset, impact)
        let data = vld1q_u8(inner_ptr as *const u8);
        
        // Extract offsets (first 8 bytes)
        let offsets = vandq_u8(data, vdupq_n_u8(0xFF));
        
        // Extract impacts (second 8 bytes) - shift and mask
        let impacts_shifted = vshrq_n_u8(data, 8);
        let impacts = vandq_u8(impacts_shifted, vdupq_n_u8(0xFF));
        
        // Convert to u16 for multiplication
        let impacts_u16 = vmovl_u8(impacts);
        let scaled_impacts = vmulq_u16(impacts_u16, value_vec);
        
        // Process each element using unrolled loop
        let offset0 = vgetq_lane_u8(offsets, 0) as usize;
        let impact0 = vgetq_lane_u16(scaled_impacts, 0);
        if offset0 < doc_score.len() { doc_score[offset0] += impact0; }
        
        let offset1 = vgetq_lane_u8(offsets, 1) as usize;
        let impact1 = vgetq_lane_u16(scaled_impacts, 1);
        if offset1 < doc_score.len() { doc_score[offset1] += impact1; }
        
        let offset2 = vgetq_lane_u8(offsets, 2) as usize;
        let impact2 = vgetq_lane_u16(scaled_impacts, 2);
        if offset2 < doc_score.len() { doc_score[offset2] += impact2; }
        
        let offset3 = vgetq_lane_u8(offsets, 3) as usize;
        let impact3 = vgetq_lane_u16(scaled_impacts, 3);
        if offset3 < doc_score.len() { doc_score[offset3] += impact3; }
        
        let offset4 = vgetq_lane_u8(offsets, 4) as usize;
        let impact4 = vgetq_lane_u16(scaled_impacts, 4);
        if offset4 < doc_score.len() { doc_score[offset4] += impact4; }
        
        let offset5 = vgetq_lane_u8(offsets, 5) as usize;
        let impact5 = vgetq_lane_u16(scaled_impacts, 5);
        if offset5 < doc_score.len() { doc_score[offset5] += impact5; }
        
        let offset6 = vgetq_lane_u8(offsets, 6) as usize;
        let impact6 = vgetq_lane_u16(scaled_impacts, 6);
        if offset6 < doc_score.len() { doc_score[offset6] += impact6; }
        
        let offset7 = vgetq_lane_u8(offsets, 7) as usize;
        let impact7 = vgetq_lane_u16(scaled_impacts, 7);
        if offset7 < doc_score.len() { doc_score[offset7] += impact7; }
        
        inner_ptr = inner_ptr.add(8);
    }
    
    // Handle remaining elements
    while inner_ptr != end_inner_ptr {
        doc_score[(*inner_ptr).0 as usize] += (value as u16) * ((*inner_ptr).1 as u16);
        inner_ptr = inner_ptr.add(1);
    }
}
