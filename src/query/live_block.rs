#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::*;

#[inline]
pub fn compute_upper_bounds_raw(
    query_ranges: &[&[u8]],
    query_weights: &[u8],
    vector_len: usize,
) -> Vec<u16> {
    let mut upper_bounds: Vec<u16> = vec![0; vector_len];

    // Iterate over each vector in scores and add its elements to the result
    for (&vec, &weight) in query_ranges.iter().zip(query_weights.iter()) {
        unsafe {
            #[cfg(target_arch = "x86_64")]
            {
                if is_x86_feature_detected!("avx2") && vec.len() >= 16 {
                    compute_upper_bounds_simd_x86_64(&mut upper_bounds, vec, weight);
                } else {
                    compute_upper_bounds_scalar(&mut upper_bounds, vec, weight);
                }
            }
            
            #[cfg(target_arch = "aarch64")]
            {
                if std::arch::is_aarch64_feature_detected!("neon") && vec.len() >= 16 {
                    compute_upper_bounds_simd_aarch64(&mut upper_bounds, vec, weight);
                } else {
                    compute_upper_bounds_scalar(&mut upper_bounds, vec, weight);
                }
            }
            
            #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
            {
                compute_upper_bounds_scalar(&mut upper_bounds, vec, weight);
            }
        }
    }
    upper_bounds
}

#[inline]
unsafe fn compute_upper_bounds_scalar(
    upper_bounds: &mut [u16],
    vec: &[u8],
    weight: u8,
) {
    for (i, &score) in vec.iter().enumerate() {
        if i < upper_bounds.len() {
            let multiplied = score as u16 * weight as u16;
            upper_bounds[i] = upper_bounds[i].saturating_add(multiplied);
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[inline]
unsafe fn compute_upper_bounds_simd_x86_64(
    upper_bounds: &mut [u16],
    vec: &[u8],
    weight: u8,
) {
    let weight_vec = _mm256_set1_epi16(weight as i16);
    let mut i = 0;
    
    // Process 16 elements at a time
    while i + 16 <= vec.len() && i + 16 <= upper_bounds.len() {
        // Load 16 u8 scores
        let scores_u8 = _mm_loadu_si128(vec.as_ptr().add(i) as *const __m128i);
        
        // Convert to u16 (zero-extend)
        let scores_low = _mm256_unpacklo_epi8(_mm256_castsi128_si256(scores_u8), _mm256_setzero_si256());
        let scores_high = _mm256_unpackhi_epi8(_mm256_castsi128_si256(scores_u8), _mm256_setzero_si256());
        
        // Multiply by weight
        let scaled_low = _mm256_mullo_epi16(scores_low, weight_vec);
        let scaled_high = _mm256_mullo_epi16(scores_high, weight_vec);
        
        // Load existing upper bounds
        let bounds_low = _mm256_loadu_si256(upper_bounds.as_ptr().add(i) as *const __m256i);
        let bounds_high = _mm256_loadu_si256(upper_bounds.as_ptr().add(i + 8) as *const __m256i);
        
        // Add with saturation
        let result_low = _mm256_adds_epu16(bounds_low, scaled_low);
        let result_high = _mm256_adds_epu16(bounds_high, scaled_high);
        
        // Store results
        _mm256_storeu_si256(upper_bounds.as_mut_ptr().add(i) as *mut __m256i, result_low);
        _mm256_storeu_si256(upper_bounds.as_mut_ptr().add(i + 8) as *mut __m256i, result_high);
        
        i += 16;
    }
    
    // Handle remaining elements
    while i < vec.len() && i < upper_bounds.len() {
        let multiplied = vec[i] as u16 * weight as u16;
        upper_bounds[i] = upper_bounds[i].saturating_add(multiplied);
        i += 1;
    }
}

#[cfg(target_arch = "aarch64")]
#[inline]
unsafe fn compute_upper_bounds_simd_aarch64(
    upper_bounds: &mut [u16],
    vec: &[u8],
    weight: u8,
) {
    let weight_vec = vdupq_n_u16(weight as u16);
    let mut i = 0;
    
    // Process 16 elements at a time
    while i + 16 <= vec.len() && i + 16 <= upper_bounds.len() {
        // Load 16 u8 scores
        let scores_u8 = vld1q_u8(vec.as_ptr().add(i));
        
        // Convert to u16 (zero-extend)
        let scores_low = vmovl_u8(vget_low_u8(scores_u8));
        let scores_high = vmovl_u8(vget_high_u8(scores_u8));
        
        // Multiply by weight
        let scaled_low = vmulq_u16(scores_low, weight_vec);
        let scaled_high = vmulq_u16(scores_high, weight_vec);
        
        // Load existing upper bounds
        let bounds_low = vld1q_u16(upper_bounds.as_ptr().add(i));
        let bounds_high = vld1q_u16(upper_bounds.as_ptr().add(i + 8));
        
        // Add with saturation
        let result_low = vqaddq_u16(bounds_low, scaled_low);
        let result_high = vqaddq_u16(bounds_high, scaled_high);
        
        // Store results
        vst1q_u16(upper_bounds.as_mut_ptr().add(i), result_low);
        vst1q_u16(upper_bounds.as_mut_ptr().add(i + 8), result_high);
        
        i += 16;
    }
    
    // Handle remaining elements
    while i < vec.len() && i < upper_bounds.len() {
        let multiplied = vec[i] as u16 * weight as u16;
        upper_bounds[i] = upper_bounds[i].saturating_add(multiplied);
        i += 1;
    }
}

#[inline]
pub fn compute_upper_bounds(
    query_ranges: &[&[crate::index::posting_list::CompressedBlock]],
    query_weights: &[u8],
    vector_len: usize,
) -> Vec<u16> {
    // let vector_len: usize = query_ranges[0].len();
    let mut upper_bounds: Vec<u16> = vec![0; vector_len];

    // Iterate over each vector in scores and add its elements to the result
    for (&vec, &weight) in query_ranges.iter().zip(query_weights.iter()) {
        for (bid, block) in vec.iter().enumerate() {
            for &(offset, score) in &block.max_scores {
                let multiplied = score as u16 * weight as u16;
                upper_bounds[bid * 256 + offset] =
                    upper_bounds[bid * 256 + offset].saturating_add(multiplied);
            }
        }
    }
    upper_bounds
}
