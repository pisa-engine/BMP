use blas::saxpy;
use rayon::prelude::*;

#[inline]
pub fn compute_upper_bounds_raw_blas(
    query_ranges: &[&[u8]],
    query_weights: &[u8],
    vector_len: usize,
) -> Vec<u16> {
    let mut upper_bounds = vec![0.0f32; vector_len];
    
    // Use BLAS SAXPY (single precision a*x + y) for vectorized computation
    for (&vec, &weight) in query_ranges.iter().zip(query_weights.iter()) {
        let weight_f32 = weight as f32;
        let x: Vec<f32> = vec.iter().map(|&v| v as f32).collect();
        
        unsafe {
            saxpy(
                vector_len as i32,
                weight_f32,
                &x,
                1,
                &mut upper_bounds,
                1,
            );
        }
    }
    
    // Convert back to u16 with saturation
    upper_bounds.into_iter()
        .map(|x| (x.min(u16::MAX as f32) as u16))
        .collect()
}


#[inline]
pub fn compute_upper_bounds_raw(
    query_ranges: &[&[u8]],
    query_weights: &[u8],
    vector_len: usize,
) -> Vec<u16> {
    // Use BLAS only for very large vectors where the overhead is justified
    if vector_len > 100000 && query_ranges.len() > 10 {
        compute_upper_bounds_raw_blas(query_ranges, query_weights, vector_len)
    } else {
        // Original implementation for normal vectors - much faster for typical cases
        let mut upper_bounds: Vec<u16> = vec![0; vector_len];
        for (&vec, &weight) in query_ranges.iter().zip(query_weights.iter()) {
            for (i, &score) in vec.iter().enumerate() {
                let multiplied = score as u16 * weight as u16;
                upper_bounds[i] = upper_bounds[i].saturating_add(multiplied);
            }
        }
        upper_bounds
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
