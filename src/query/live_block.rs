#[inline]
pub fn compute_upper_bounds_raw(
    query_ranges: &[&[u8]],
    query_weights: &[u8],
    vector_len: usize,
) -> Vec<u16> {
    // let vector_len: usize = query_ranges[0].len();
    let mut upper_bounds: Vec<u16> = vec![0; vector_len];

    // Iterate over each vector in scores and add its elements to the result
    for (&vec, &weight) in query_ranges.iter().zip(query_weights.iter()) {
        for (i, &score) in vec.iter().enumerate() {
            let multiplied = score as u16 * weight as u16;
            upper_bounds[i] = upper_bounds[i].saturating_add(multiplied);
        }
    }
    upper_bounds
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
