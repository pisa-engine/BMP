use anyhow::Result;

use bmp::index::forward_index::BlockForwardIndex;
use bmp::index::posting_list::PostingListIterator;
use bmp::query::cursor::DocId;
use bmp::query::cursor::{RangeMaxScore, RangeMaxScoreCursor};
use bmp::query::cursors_from_queries;
use bmp::query::live_block;
use bmp::query::topk_heap::TopKHeap;
use std::arch::x86_64::_mm_prefetch;
use std::time::Instant;

use std::path::PathBuf;
use structopt::StructOpt;

// Function to perform the search for each query and return the results
fn b_search(
    mut queries: Vec<Vec<PostingListIterator>>,
    forward_index: &BlockForwardIndex,
    k: usize,
    bsize: usize,
    alpha: f32,
    terms_r: f32,
) -> Vec<TopKHeap<u16>> {
    let mut results: Vec<TopKHeap<u16>> = Vec::new();
    let progress = bmp::util::progress_bar("Forward index-based search", queries.len());

    let mut search_elapsed = 0;
    let mut total_scores = 0;
    let mut compute_ub_time = 0;
    let mut fwd_time = 0;
    let mut topk_time = 0;
    let mut sort_elapsed = 0;
    let mut query_ranges_time = 0;
    let mut buckets: Vec<Vec<u32>> = (0..=2usize.pow(16)).map(|_| Vec::new()).collect();

    for query in queries.iter_mut() {
        let total_terms = query.len();
        let terms_to_keep = (total_terms as f32 * terms_r).ceil() as usize;
        query.sort_by(|a, b| b.term_weight().partial_cmp(&a.term_weight()).unwrap());
        // Keep only the top N terms
        query.truncate(terms_to_keep);

        let query_weights: Vec<_> = query.iter().map(|post| post.term_weight()).collect();

        let start_query_ranges: Instant = Instant::now();
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
        query_ranges_time += start_query_ranges.elapsed().as_micros();

        let start_search: Instant = Instant::now();
        let start_ub: Instant = Instant::now();
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
        compute_ub_time += start_ub.elapsed().as_micros();

        let mut topk = TopKHeap::with_threshold(k, threshold as u16);
        let start_sort: Instant = Instant::now();
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

        sort_elapsed += start_sort.elapsed().as_micros();

        let (mut current_ub, mut current_block) = ub_iter.next().unwrap();
        unsafe {
            _mm_prefetch(
                forward_index.data.as_ptr().add(*current_block as usize) as *const i8,
                std::arch::x86_64::_MM_HINT_T0,
            );
        }
        for (next_ub, next_block) in ub_iter {
            unsafe {
                _mm_prefetch(
                    forward_index.data.as_ptr().add(*next_block as usize) as *const i8,
                    std::arch::x86_64::_MM_HINT_T0,
                );
            }

            let offset = *current_block as usize * bsize;

            let start_fwd: Instant = Instant::now();
            let res = bmp::index::forward_index::block_score(
                &query_vec,
                &forward_index.data[*current_block as usize],
                bsize,
            );
            fwd_time += start_fwd.elapsed().as_micros();

            total_scores += 1;
            let start_topk: Instant = Instant::now();
            for (doc_id, &score) in res.iter().enumerate() {
                topk.insert(DocId(doc_id as u32 + offset as u32), score);
            }
            topk_time += start_topk.elapsed().as_micros();

            if topk.threshold() as f32 > current_ub as f32 * alpha {
                break;
            }
            current_block = next_block;
            current_ub = next_ub;
        }
        search_elapsed += start_search.elapsed().as_micros();
        results.push(topk.clone());
        progress.inc(1);
    }
    progress.finish();

    eprintln!(
        "search_elapsed = {}",
        search_elapsed / results.len() as u128
    );

    results
}
// Function to convert query results to TREC format and print to stdout
fn to_trec(query_ids: &[String], mut results: Vec<TopKHeap<u16>>, doc_lexicon: &[String]) {
    for (id, result) in query_ids.iter().zip(results.iter_mut()) {
        for (rank, r) in result.to_sorted_vec().iter().enumerate() {
            println!(
                "{} Q0 {} {} {} BMP",
                id,
                doc_lexicon[r.doc_id.0 as usize],
                rank + 1,
                r.score,
            );
        }
    }
}

#[derive(Debug, StructOpt)]
#[structopt(name = "search", about = "Search an index and produce a TREC output")]

struct Args {
    #[structopt(short, long, help = "Path to the index")]
    index: PathBuf,
    #[structopt(short, long, help = "Path to the queries")]
    queries: PathBuf,
    #[structopt(short, long, help = "Number of documents to retrieve")]
    k: usize,
    #[structopt(short, long, help = "Block size")]
    bsize: usize,
    #[structopt(short, long, help = "approximation factor", default_value = "1.0")]
    alpha: f32,
    #[structopt(
        short,
        long,
        help = "terms approximation factor",
        default_value = "1.0"
    )]
    beta: f32,
}
fn main() -> Result<()> {
    let args = Args::from_args();

    // 1. Load the index
    eprintln!("Loading the index");
    let (index, bfwd) = bmp::index::from_file(args.index)?;

    // 2. Load the queries
    eprintln!("Loading the queries");
    let (q_ids, cursors) = cursors_from_queries(args.queries, &index);

    eprintln!("Performing query processing");
    let results = b_search(cursors, &bfwd, args.k, args.bsize, args.alpha, args.beta);

    eprintln!("Exporting TREC run");
    // 4. Log results into TREC format
    to_trec(&q_ids, results, index.documents());
    Ok(())
}
