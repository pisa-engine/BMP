use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::prelude::*;
use rand::seq::SliceRandom;

// Import the block_score function from the library
use bmp::index::forward_index::block_score;

/// Generate realistic test data for benchmarking
struct TestData {
    query: Vec<(u16, u8)>,
    document: Vec<(u16, (Vec<u8>, Vec<u8>))>,
    block_size: usize,
}

impl TestData {
    /// Create test data with specified parameters
    fn new(
        query_length: usize,
        num_terms: usize,
        avg_postings_per_term: usize,
        block_size: usize,
    ) -> Self {
        let mut rng = StdRng::seed_from_u64(42); // Fixed seed for reproducibility

        // Generate query terms with weights
        let mut query_terms: Vec<u16> = (0..num_terms).map(|i| i as u16).collect();
        query_terms.shuffle(&mut rng);
        query_terms.truncate(query_length);

        let query = query_terms
            .into_iter()
            .map(|term_id| (term_id, rng.gen_range(1..=255)))
            .collect();

        // Generate document structure
        let mut document = Vec::new();
        for term_id in 0..num_terms {
            let num_postings = rng.gen_range(1..=avg_postings_per_term * 2);
            let mut doc_ids = Vec::new();
            let mut scores = Vec::new();

            for _ in 0..num_postings {
                let doc_id = rng.gen_range(0..block_size);
                let score = rng.gen_range(1..=255);
                doc_ids.push(doc_id as u8);
                scores.push(score);
            }

            if !doc_ids.is_empty() {
                document.push((term_id as u16, (doc_ids, scores)));
            }
        }

        // Sort document by term_id for realistic structure
        document.sort_by_key(|(term_id, _)| *term_id);

        TestData {
            query,
            document,
            block_size,
        }
    }

    /// Create sparse test data (fewer matches)
    fn sparse(query_length: usize, block_size: usize) -> Self {
        Self::new(query_length, 1000, 2, block_size)
    }

    /// Create dense test data (more matches)
    fn dense(query_length: usize, block_size: usize) -> Self {
        Self::new(query_length, 100, 20, block_size)
    }

    /// Create balanced test data
    fn balanced(query_length: usize, block_size: usize) -> Self {
        Self::new(query_length, 500, 4, block_size)
    }
}

fn benchmark_block_score_varying_block_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_score_block_sizes");
    
    let block_sizes = [16, 32, 64, 128, 256];
    let query_length = 50;
    
    for &block_size in &block_sizes {
        let test_data = TestData::balanced(query_length, block_size);
        
        group.throughput(Throughput::Elements(block_size as u64));
        group.bench_with_input(
            BenchmarkId::new("balanced", block_size),
            &test_data,
            |b, data| {
                b.iter(|| {
                    block_score(
                        black_box(&data.query),
                        black_box(&data.document),
                        black_box(data.block_size),
                    )
                })
            },
        );
    }
    group.finish();
}

fn benchmark_block_score_varying_query_lengths(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_score_query_lengths");
    
    let query_lengths = [10, 25, 50, 100, 200];
    let block_size = 64;
    
    for &query_length in &query_lengths {
        let test_data = TestData::balanced(query_length, block_size);
        
        group.throughput(Throughput::Elements(query_length as u64));
        group.bench_with_input(
            BenchmarkId::new("balanced", query_length),
            &test_data,
            |b, data| {
                b.iter(|| {
                    block_score(
                        black_box(&data.query),
                        black_box(&data.document),
                        black_box(data.block_size),
                    )
                })
            },
        );
    }
    group.finish();
}

fn benchmark_block_score_data_sparsity(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_score_data_sparsity");
    
    let query_length = 50;
    let block_size = 64;
    
    let sparse_data = TestData::sparse(query_length, block_size);
    let balanced_data = TestData::balanced(query_length, block_size);
    let dense_data = TestData::dense(query_length, block_size);
    
    group.bench_function("sparse", |b| {
        b.iter(|| {
            block_score(
                black_box(&sparse_data.query),
                black_box(&sparse_data.document),
                black_box(sparse_data.block_size),
            )
        })
    });
    
    group.bench_function("balanced", |b| {
        b.iter(|| {
            block_score(
                black_box(&balanced_data.query),
                black_box(&balanced_data.document),
                black_box(balanced_data.block_size),
            )
        })
    });
    
    group.bench_function("dense", |b| {
        b.iter(|| {
            block_score(
                black_box(&dense_data.query),
                black_box(&dense_data.document),
                black_box(dense_data.block_size),
            )
        })
    });
    
    group.finish();
}

fn benchmark_block_score_extreme_cases(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_score_extreme_cases");
    
    // Very small block
    let tiny_data = TestData::balanced(10, 8);
    group.bench_function("tiny_block", |b| {
        b.iter(|| {
            block_score(
                black_box(&tiny_data.query),
                black_box(&tiny_data.document),
                black_box(tiny_data.block_size),
            )
        })
    });
    
    // Large block
    let large_data = TestData::balanced(100, 512);
    group.bench_function("large_block", |b| {
        b.iter(|| {
            block_score(
                black_box(&large_data.query),
                black_box(&large_data.document),
                black_box(large_data.block_size),
            )
        })
    });
    
    // Very long query
    let long_query_data = TestData::balanced(500, 64);
    group.bench_function("long_query", |b| {
        b.iter(|| {
            block_score(
                black_box(&long_query_data.query),
                black_box(&long_query_data.document),
                black_box(long_query_data.block_size),
            )
        })
    });
    
    // Very short query
    let short_query_data = TestData::balanced(5, 64);
    group.bench_function("short_query", |b| {
        b.iter(|| {
            block_score(
                black_box(&short_query_data.query),
                black_box(&short_query_data.document),
                black_box(short_query_data.block_size),
            )
        })
    });
    
    group.finish();
}

fn benchmark_block_score_no_matches(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_score_no_matches");
    
    let mut rng = StdRng::seed_from_u64(42);
    let block_size = 64;
    
    // Query with terms that don't exist in document
    let query: Vec<(u16, u8)> = (1000..1050)
        .map(|term_id| (term_id, rng.gen_range(1..=255)))
        .collect();
    
    // Document with completely different terms
    let document: Vec<(u16, (Vec<u8>, Vec<u8>))> = (0..100)
        .map(|term_id| {
            let mut doc_ids = Vec::new();
            let mut scores = Vec::new();
            for _ in 0..5 {
                let doc_id = rng.gen_range(0..block_size);
                let score = rng.gen_range(1..=255);
                doc_ids.push(doc_id as u8);
                scores.push(score);
            }
            (term_id as u16, (doc_ids, scores))
        })
        .collect();
    
    group.bench_function("no_matches", |b| {
        b.iter(|| {
            block_score(
                black_box(&query),
                black_box(&document),
                black_box(block_size),
            )
        })
    });
    
    group.finish();
}

fn benchmark_block_score_perfect_matches(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_score_perfect_matches");
    
    let mut rng = StdRng::seed_from_u64(42);
    let block_size = 64;
    let query_length = 50;
    
    // Generate query terms
    let query_terms: Vec<u16> = (0..query_length).map(|i| i as u16).collect();
    let query: Vec<(u16, u8)> = query_terms
        .into_iter()
        .map(|term_id| (term_id, rng.gen_range(1..=255)))
        .collect();
    
    // Generate document with exact same terms
    let document: Vec<(u16, (Vec<u8>, Vec<u8>))> = query
        .iter()
        .map(|(term_id, _)| {
            let mut doc_ids = Vec::new();
            let mut scores = Vec::new();
            for _ in 0..10 {
                let doc_id = rng.gen_range(0..block_size);
                let score = rng.gen_range(1..=255);
                doc_ids.push(doc_id as u8);
                scores.push(score);
            }
            (*term_id, (doc_ids, scores))
        })
        .collect();
    
    group.bench_function("perfect_matches", |b| {
        b.iter(|| {
            block_score(
                black_box(&query),
                black_box(&document),
                black_box(block_size),
            )
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    benchmark_block_score_varying_block_sizes,
    benchmark_block_score_varying_query_lengths,
    benchmark_block_score_data_sparsity,
    benchmark_block_score_extreme_cases,
    benchmark_block_score_no_matches,
    benchmark_block_score_perfect_matches
);

criterion_main!(benches);
