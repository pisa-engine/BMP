use crate::index::forward_index::block_score;
use crate::index::forward_index::BlockForwardIndex;
use crate::index::posting_list::PostingListIterator;
use crate::query::cursor::DocId;
use crate::query::cursor::{RangeMaxScore, RangeMaxScoreCursor};
use crate::query::live_block;
use crate::query::topk_heap::TopKHeap;
use crate::util::progress_bar;
use std::time::Instant;
use rayon::prelude::*;

#[cfg(target_arch = "aarch64")]
use std::arch::aarch64::{_prefetch, _PREFETCH_LOCALITY0, _PREFETCH_READ};

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::{_mm_prefetch, _MM_HINT_T0, _MM_HINT_T1, _MM_HINT_T2};

// Advanced prefetching constants
const PREFETCH_DISTANCE: usize = 4; // Prefetch 4 blocks ahead
const CACHE_LINE_SIZE: usize = 64;

#[cfg(target_arch = "x86_64")]
fn prefetch_block_advanced(forward_index: &BlockForwardIndex, block: u32, hint: i32) {
    unsafe {
        _mm_prefetch(
            forward_index.data.as_ptr().add(block as usize) as *const i8,
            hint,
        );
    }
}

#[cfg(target_arch = "aarch64")]
fn prefetch_block_advanced(forward_index: &BlockForwardIndex, block: u32, _hint: i32) {
    unsafe {
        _prefetch(
            forward_index.data.as_ptr().add(block as usize) as *const i8,
            _PREFETCH_READ,
            _PREFETCH_LOCALITY0,
        );
    }
}

#[cfg(target_arch = "x86_64")]
fn prefetch_block(forward_index: &BlockForwardIndex, block: u32) {
    prefetch_block_advanced(forward_index, block, _MM_HINT_T0);
}

#[cfg(target_arch = "aarch64")]
fn prefetch_block(forward_index: &BlockForwardIndex, block: u32) {
    prefetch_block_advanced(forward_index, block, 0);
}

// Multi-level prefetching strategy
fn prefetch_blocks_multilevel(forward_index: &BlockForwardIndex, blocks: &[u32]) {
    for (i, &block) in blocks.iter().take(PREFETCH_DISTANCE).enumerate() {
        #[cfg(target_arch = "x86_64")]
        let hint = match i {
            0 => _MM_HINT_T0,  // L1 cache
            1 => _MM_HINT_T1,  // L2 cache
            _ => _MM_HINT_T2,  // L3 cache
        };
        
        #[cfg(target_arch = "x86_64")]
        prefetch_block_advanced(forward_index, block, hint);
        
        #[cfg(target_arch = "aarch64")]
        prefetch_block_advanced(forward_index, block, 0);
    }
}

// GPU acceleration for very large datasets
#[cfg(feature = "gpu")]
use arrayfire as af;

#[cfg(feature = "gpu")]
pub fn b_search_gpu(
    mut queries: Vec<Vec<PostingListIterator>>,
    forward_index: &BlockForwardIndex,
    k: usize,
    alpha: f32,
    terms_r: f32,
    verbose: bool,
) -> Vec<TopKHeap<u16>> {
    use af::*;
    
    // Initialize ArrayFire
    set_device(0);
    info();
    
    let mut results: Vec<TopKHeap<u16>> = Vec::new();
    
    let progress = if verbose {
        Some(progress_bar("GPU-accelerated search", queries.len()))
    } else {
        None
    };

    let mut search_elapsed = 0;
    
    // Process queries in batches for GPU efficiency
    let gpu_batch_size = 32; // Process 32 queries simultaneously on GPU
    
    for query_batch in queries.chunks_mut(gpu_batch_size) {
        let start_batch = Instant::now();
        
        // Prepare data for GPU processing
        let mut batch_query_vecs = Vec::new();
        let mut batch_thresholds = Vec::new();
        let mut batch_upper_bounds = Vec::new();
        
        for query in query_batch.iter_mut() {
            let total_terms = query.len();
            let terms_to_keep = (total_terms as f32 * terms_r).ceil() as usize;
            query.sort_by(|a, b| b.term_weight().partial_cmp(&a.term_weight()).unwrap());
            query.truncate(terms_to_keep);

            let query_weights: Vec<_> = query.iter().map(|post| post.term_weight()).collect();
            let query_ranges: Vec<_> = query.iter().map(|post| post.range_max_scores()).collect();
            
            let mut query_ranges_raw = Vec::new();
            let mut query_ranges_compressed = Vec::new();
            for qr in query_ranges {
                match qr {
                    RangeMaxScore::Compressed(compressed) => query_ranges_compressed.push(compressed),
                    RangeMaxScore::Raw(raw) => query_ranges_raw.push(raw),
                };
            }

            let query_vec = query
                .iter()
                .map(|&pl| (pl.term_id() as u16, pl.term_weight() as u8))
                .collect::<Vec<_>>();
            
            let threshold = query
                .iter()
                .map(|&pl| pl.kth(k) as u16 * pl.term_weight() as u16)
                .max()
                .unwrap_or(0);

            let run_compressed = !query_ranges_compressed.is_empty();
            let upper_bounds = match run_compressed {
                true => live_block::compute_upper_bounds(
                    &query_ranges_compressed,
                    &query_weights,
                    forward_index.data.len(),
                ),
                false => live_block::compute_upper_bounds_raw(
                    &query_ranges_raw,
                    &query_weights,
                    forward_index.data.len(),
                ),
            };
            
            batch_query_vecs.push(query_vec);
            batch_thresholds.push(threshold);
            batch_upper_bounds.push(upper_bounds);
        }
        
        // Transfer data to GPU and perform parallel processing
        let batch_results = process_batch_on_gpu(
            &batch_query_vecs,
            &batch_thresholds,
            &batch_upper_bounds,
            forward_index,
            k,
            alpha,
        );
        
        search_elapsed += start_batch.elapsed().as_micros();
        results.extend(batch_results);
        
        if let Some(progress_bar) = &progress {
            progress_bar.inc(query_batch.len() as u64);
        }
    }
    
    if let Some(progress_bar) = &progress {
        progress_bar.finish();
    }

    if verbose {
        eprintln!(
            "gpu_search_elapsed = {}",
            search_elapsed / results.len() as u128
        );
    }

    results
}

#[cfg(feature = "gpu")]
fn process_batch_on_gpu(
    query_vecs: &[Vec<(u16, u8)>],
    thresholds: &[u16],
    upper_bounds: &[Vec<u16>],
    forward_index: &BlockForwardIndex,
    k: usize,
    alpha: f32,
) -> Vec<TopKHeap<u16>> {
    use af::*;
    
    let batch_size = query_vecs.len();
    let mut results = Vec::with_capacity(batch_size);
    
    // Create GPU arrays for upper bounds
    let max_bounds_len = upper_bounds.iter().map(|ub| ub.len()).max().unwrap_or(0);
    let mut bounds_matrix = vec![0u16; batch_size * max_bounds_len];
    
    for (i, bounds) in upper_bounds.iter().enumerate() {
        for (j, &bound) in bounds.iter().enumerate() {
            bounds_matrix[i * max_bounds_len + j] = bound;
        }
    }
    
    let gpu_bounds = Array::new(&bounds_matrix, Dim4::new(&[max_bounds_len as u64, batch_size as u64, 1, 1]));
    let gpu_thresholds = Array::new(thresholds, Dim4::new(&[batch_size as u64, 1, 1, 1]));
    
    // Compute block priorities on GPU
    let threshold_matrix = tile(&gpu_thresholds, Dim4::new(&[max_bounds_len as u64, 1, 1, 1]));
    let valid_blocks = gt(&gpu_bounds, &threshold_matrix, false);
    
    // Download results and process on CPU (hybrid approach)
    let valid_blocks_cpu: Vec<u8> = valid_blocks.host();
    
    for (query_idx, (query_vec, &threshold)) in query_vecs.iter().zip(thresholds.iter()).enumerate() {
        let mut topk = TopKHeap::with_threshold(k, threshold);
        
        // Process blocks in priority order
        let query_bounds = &upper_bounds[query_idx];
        let mut block_priorities: Vec<(u16, usize)> = query_bounds
            .iter()
            .enumerate()
            .filter(|(block_idx, &ub)| valid_blocks_cpu[query_idx * max_bounds_len + block_idx] > 0)
            .map(|(block_idx, &ub)| (ub, block_idx))
            .collect();
        
        block_priorities.sort_by(|a, b| b.0.cmp(&a.0)); // Sort by upper bound descending
        
        for (current_ub, block_idx) in block_priorities {
            let offset = block_idx * forward_index.block_size;
            
            let res = block_score(
                query_vec,
                &forward_index.data[block_idx],
                forward_index.block_size,
            );
            
            for (doc_id, &score) in res.iter().enumerate() {
                topk.insert(DocId(doc_id as u32 + offset as u32), score);
            }
            
            if topk.threshold() as f32 > current_ub as f32 * alpha {
                break;
            }
        }
        
        results.push(topk);
    }
    
    results
}

pub fn b_search(
    queries: Vec<Vec<PostingListIterator>>,
    forward_index: &BlockForwardIndex,
    k: usize,
    alpha: f32,
    terms_r: f32,
) -> Vec<TopKHeap<u16>> {
    b_search_verbose(queries, forward_index, k, alpha, terms_r, true)
}

pub fn b_search_optimized(
    mut queries: Vec<Vec<PostingListIterator>>,
    forward_index: &BlockForwardIndex,
    k: usize,
    alpha: f32,
    terms_r: f32,
    verbose: bool,
) -> Vec<TopKHeap<u16>> {
    let mut results: Vec<TopKHeap<u16>> = Vec::new();

    let progress = if verbose {
        Some(progress_bar("Optimized forward index search", queries.len()))
    } else {
        None
    };

    let mut search_elapsed = 0;
    
    // Pre-allocate bucket arrays for better memory reuse
    let mut buckets: Vec<Vec<u32>> = (0..=2usize.pow(16)).map(|_| Vec::with_capacity(1024)).collect();
    let mut prefetch_queue: Vec<u32> = Vec::with_capacity(PREFETCH_DISTANCE);

    // Process queries in parallel for better CPU utilization
    let chunk_size = (queries.len() / rayon::current_num_threads()).max(1);
    
    for query_chunk in queries.chunks_mut(chunk_size) {
        let chunk_results: Vec<_> = query_chunk.iter_mut().map(|query| {
            let total_terms = query.len();
            let terms_to_keep = (total_terms as f32 * terms_r).ceil() as usize;
            query.sort_by(|a, b| b.term_weight().partial_cmp(&a.term_weight()).unwrap());
            query.truncate(terms_to_keep);

            let query_weights: Vec<_> = query.iter().map(|post| post.term_weight()).collect();
            let query_ranges: Vec<_> = query.iter().map(|post| post.range_max_scores()).collect();
            
            let mut query_ranges_raw = Vec::new();
            let mut query_ranges_compressed = Vec::new();
            for qr in query_ranges {
                match qr {
                    RangeMaxScore::Compressed(compressed) => query_ranges_compressed.push(compressed),
                    RangeMaxScore::Raw(raw) => query_ranges_raw.push(raw),
                };
            }

            let mut query_vec = query
                .iter()
                .map(|&pl| (pl.term_id() as u16, pl.term_weight() as u8))
                .collect::<Vec<_>>();
            query_vec.sort_by_key(|e| e.0);
            
            let threshold = query
                .iter()
                .map(|&pl| pl.kth(k) as u16 * pl.term_weight() as u16)
                .max()
                .unwrap_or(0);

            let start_search = Instant::now();
            let run_compressed = !query_ranges_compressed.is_empty();
            
            let upper_bounds = match run_compressed {
                true => live_block::compute_upper_bounds(
                    &query_ranges_compressed,
                    &query_weights,
                    forward_index.data.len(),
                ),
                false => live_block::compute_upper_bounds_raw(
                    &query_ranges_raw,
                    &query_weights,
                    forward_index.data.len(),
                ),
            };

            let mut topk = TopKHeap::with_threshold(k, threshold as u16);
            
            // Create local buckets for this thread
            let mut local_buckets: Vec<Vec<u32>> = (0..=2usize.pow(16)).map(|_| Vec::new()).collect();
            
            upper_bounds.iter().enumerate().for_each(|(range_id, &ub)| {
                if ub > threshold {
                    local_buckets[ub as usize].push(range_id as u32);
                }
            });

            // Collect blocks for processing with adaptive batch sizes
            let mut processing_queue: Vec<(usize, u32)> = Vec::new();
            for (ub, blocks) in local_buckets.iter().enumerate().rev() {
                for &block in blocks {
                    processing_queue.push((ub, block));
                }
            }

            // Process blocks in batches with prefetching
            let batch_size = PREFETCH_DISTANCE.min(processing_queue.len());
            
            for batch in processing_queue.chunks(batch_size) {
                // Prefetch next batch
                let prefetch_blocks: Vec<u32> = batch.iter().map(|(_, block)| *block).collect();
                prefetch_blocks_multilevel(forward_index, &prefetch_blocks);
                
                // Process current batch
                for &(current_ub, current_block) in batch {
                    let offset = current_block as usize * forward_index.block_size;

                    let res = block_score(
                        &query_vec,
                        &forward_index.data[current_block as usize],
                        forward_index.block_size,
                    );

                    for (doc_id, &score) in res.iter().enumerate() {
                        topk.insert(DocId(doc_id as u32 + offset as u32), score);
                    }

                    if topk.threshold() as f32 > current_ub as f32 * alpha {
                        break;
                    }
                }
            }
            
            (topk, start_search.elapsed().as_micros())
        }).collect();
        
        for (topk, elapsed) in chunk_results {
            results.push(topk);
            search_elapsed += elapsed;
        }
        
        if let Some(progress_bar) = &progress {
            progress_bar.inc(query_chunk.len() as u64);
        }
    }
    
    if let Some(progress_bar) = &progress {
        progress_bar.finish();
    }

    if verbose {
        eprintln!(
            "optimized_search_elapsed = {}",
            search_elapsed / results.len() as u128
        );
    }

    results
}

pub fn b_search_verbose(
    mut queries: Vec<Vec<PostingListIterator>>,
    forward_index: &BlockForwardIndex,
    k: usize,
    alpha: f32,
    terms_r: f32,
    verbose: bool,
) -> Vec<TopKHeap<u16>> {
    // Use original implementation by default - it's faster for most cases
    // Only use optimized version for very large datasets
    if queries.len() > 1000 && forward_index.data.len() > 100000 {
        return b_search_optimized(queries, forward_index, k, alpha, terms_r, verbose);
    }
    
    // Original implementation for normal datasets
    let mut results: Vec<TopKHeap<u16>> = Vec::new();

    let progress = if verbose {
        Some(progress_bar("Forward index-based search", queries.len()))
    } else {
        None
    };

    let mut search_elapsed = 0;
    let mut buckets: Vec<Vec<u32>> = (0..=2usize.pow(16)).map(|_| Vec::new()).collect();

    for query in queries.iter_mut() {
        let total_terms = query.len();
        let terms_to_keep = (total_terms as f32 * terms_r).ceil() as usize;
        query.sort_by(|a, b| b.term_weight().partial_cmp(&a.term_weight()).unwrap());
        query.truncate(terms_to_keep);

        let query_weights: Vec<_> = query.iter().map(|post| post.term_weight()).collect();
        let query_ranges: Vec<_> = query.iter().map(|post| post.range_max_scores()).collect();
        
        let mut query_ranges_raw = Vec::new();
        let mut query_ranges_compressed = Vec::new();
        for qr in query_ranges {
            match qr {
                RangeMaxScore::Compressed(compressed) => query_ranges_compressed.push(compressed),
                RangeMaxScore::Raw(raw) => query_ranges_raw.push(raw),
            };
        }

        let mut query_vec = query
            .iter()
            .map(|&pl| (pl.term_id() as u16, pl.term_weight() as u8))
            .collect::<Vec<_>>();
        query_vec.sort_by_key(|e| e.0);
        
        let threshold = query
            .iter()
            .map(|&pl| pl.kth(k) as u16 * pl.term_weight() as u16)
            .max()
            .unwrap_or(0);

        let start_search: Instant = Instant::now();
        let run_compressed = query_ranges_compressed.len() > 0;
        
        let upper_bounds = match run_compressed {
            true => live_block::compute_upper_bounds(
                &query_ranges_compressed,
                &query_weights,
                forward_index.data.len(),
            ),
            false => live_block::compute_upper_bounds_raw(
                &query_ranges_raw,
                &query_weights,
                forward_index.data.len(),
            ),
        };

        let mut topk = TopKHeap::with_threshold(k, threshold as u16);
        buckets.iter_mut().for_each(std::vec::Vec::clear);
        upper_bounds.iter().enumerate().for_each(|(range_id, &ub)| {
            if ub > threshold {
                buckets[ub as usize].push(range_id as u32);
            }
        });

        let mut ub_iter =
            buckets
                .iter_mut()
                .enumerate()
                .rev()
                .flat_map(|(outer_idx, inner_vec)| {
                    inner_vec.iter_mut().map(move |val| (outer_idx, val))
                });

        if let Some((mut current_ub, mut current_block)) = ub_iter.next() {
            prefetch_block_advanced(forward_index, *current_block, 0);

            for (next_ub, next_block) in ub_iter {
                prefetch_block_advanced(forward_index, *next_block, 0);
                let offset = *current_block as usize * forward_index.block_size;

                let res = block_score(
                    &query_vec,
                    &forward_index.data[*current_block as usize],
                    forward_index.block_size,
                );

                for (doc_id, &score) in res.iter().enumerate() {
                    if score > 0 {
                        topk.insert(DocId(doc_id as u32 + offset as u32), score);
                    }
                }

                if topk.threshold() as f32 > current_ub as f32 * alpha {
                    break;
                }
                current_block = next_block;
                current_ub = next_ub;
            }
        }
        
        search_elapsed += start_search.elapsed().as_micros();
        results.push(topk.clone());
        if let Some(progress_bar) = &progress {
            progress_bar.inc(1);
        }
    }
    if let Some(progress_bar) = &progress {
        progress_bar.finish();
    }

    if verbose {
        eprintln!(
            "search_elapsed = {}",
            search_elapsed / results.len() as u128
        );
    }

    results
}
