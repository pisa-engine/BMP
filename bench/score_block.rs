use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rand::prelude::*;
use rand::seq::SliceRandom;

// Import the block_score function from the library
use bmp::index::forward_index::{block_score, block_score_scalar};

/// Generate realistic test data for benchmarking
struct TestData {
    query: Vec<(u16, u8)>,
    document: Vec<(u16, (Vec<u8>, Vec<u8>))>,
    block_size: usize,
    name: String,
}

impl TestData {
    /// Create test data with specified parameters
    fn new(
        query_length: usize,
        num_terms: usize,
        avg_postings_per_term: usize,
        block_size: usize,
        name: &str,
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
            name: name.to_string(),
        }
    }

    /// Create sparse test data (fewer matches)
    fn sparse(query_length: usize, block_size: usize) -> Self {
        Self::new(query_length, 500, 2, block_size, "sparse")
    }

    /// Create dense test data (more matches)
    fn dense(query_length: usize, block_size: usize) -> Self {
        Self::new(query_length, 50, 20, block_size, "dense")
    }

    /// Create balanced test data
    fn balanced(query_length: usize, block_size: usize) -> Self {
        Self::new(query_length, 500, 4, block_size, "balanced")
    }

    fn name(&self) -> String {
        self.name.clone()
    }
}


fn benchmark_block_score_varying_query_lengths(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_score_query_lengths");
    
    let query_lengths = [5, 10, 25, 50];
    let block_size = 16;
    
    for &query_length in &query_lengths {
        for test_data in [TestData::balanced(query_length, block_size), TestData::sparse(query_length, block_size), TestData::dense(query_length, block_size)] {
            
            group.throughput(Throughput::Elements(query_length as u64));
            group.bench_with_input(
                BenchmarkId::new(format!("{}", test_data.name()), block_size),
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

            group.bench_with_input(
                BenchmarkId::new(format!("scalar_{}", test_data.name()), block_size),
                &test_data,
                |b, data| {
                    b.iter(|| {
                        block_score_scalar(
                            black_box(&data.query),
                            black_box(&data.document),
                            black_box(data.block_size),
                        )
                    })
                },
            );
        }
    }
    group.finish();
}

criterion_group!(
    benches,
    benchmark_block_score_varying_query_lengths
);

criterion_main!(benches);
