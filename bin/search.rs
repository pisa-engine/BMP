use anyhow::Result;

use bmp::query::cursors_from_queries;
use bmp::search::{b_search, b_search_optimized};
use bmp::util::to_trec;
use bmp::{PerformanceConfig, benchmark::BenchmarkSuite};
use std::path::PathBuf;
use structopt::StructOpt;

#[cfg(feature = "gpu")]
use bmp::search::b_search_gpu;

#[derive(Debug, StructOpt)]
#[structopt(name = "search", about = "Search an index and produce a TREC output with advanced optimizations")]
struct Args {
    #[structopt(short, long, help = "Path to the index")]
    index: PathBuf,
    #[structopt(short, long, help = "Path to the queries")]
    queries: PathBuf,
    #[structopt(short, long, help = "Number of documents to retrieve")]
    k: usize,
    #[structopt(short, long, help = "approximation factor", default_value = "1.0")]
    alpha: f32,
    #[structopt(
        short,
        long,
        help = "terms approximation factor",
        default_value = "1.0"
    )]
    beta: f32,
    #[structopt(long, help = "Use optimized search implementation")]
    optimized: bool,
    #[structopt(long, help = "Use GPU acceleration (requires gpu feature)")]
    gpu: bool,
    #[structopt(long, help = "Run performance benchmark")]
    benchmark: bool,
    #[structopt(long, help = "Auto-tune performance configuration")]
    auto_tune: bool,
    #[structopt(long, help = "Disable SIMD optimizations")]
    no_simd: bool,
    #[structopt(long, help = "Disable BLAS optimizations")]
    no_blas: bool,
    #[structopt(long, help = "Prefetch distance", default_value = "4")]
    prefetch_distance: usize,
}

fn main() -> Result<()> {
    let args = Args::from_args();

    // 1. Load the index
    eprintln!("Loading the index");
    let (index, bfwd) = bmp::index::from_file(args.index)?;

    // 2. Load the queries
    eprintln!("Loading the queries");
    let (q_ids, cursors) = cursors_from_queries(args.queries, &index);

    // 3. Configure performance settings
    let mut config = if args.auto_tune {
        eprintln!("Auto-tuning performance configuration...");
        PerformanceConfig::for_dataset_size(bfwd.data.len(), cursors.len())
    } else {
        PerformanceConfig {
            use_simd: !args.no_simd,
            use_blas: !args.no_blas,
            use_gpu: args.gpu,
            prefetch_distance: args.prefetch_distance,
            ..Default::default()
        }
    };

    eprintln!("Performance configuration: {:?}", config);

    // 4. Run benchmark if requested
    if args.benchmark {
        eprintln!("Running comprehensive benchmark...");
        let benchmark_suite = BenchmarkSuite::new(config.clone());
        
        // Use a subset of queries for benchmarking to save time
        let benchmark_queries = if cursors.len() > 100 {
            cursors[..100].to_vec()
        } else {
            cursors.clone()
        };
        
        let benchmark_results = benchmark_suite.run_comprehensive_benchmark(
            benchmark_queries,
            &bfwd,
            args.k,
            args.alpha,
            args.beta,
        );
        
        benchmark_suite.print_benchmark_results(&benchmark_results);
        
        // Auto-tune if requested
        if args.auto_tune {
            let mut tuning_suite = BenchmarkSuite::new(config.clone());
            config = tuning_suite.auto_tune_config(
                cursors[..10.min(cursors.len())].to_vec(),
                &bfwd,
                args.k,
                args.alpha,
                args.beta,
            );
            eprintln!("Optimized configuration: {:?}", config);
        }
    }

    // 5. Perform the actual search
    eprintln!("Performing query processing with selected optimization level");
    let results = if args.gpu {
        #[cfg(feature = "gpu")]
        {
            eprintln!("Using GPU acceleration");
            b_search_gpu(cursors, &bfwd, args.k, args.alpha, args.beta, true)
        }
        #[cfg(not(feature = "gpu"))]
        {
            eprintln!("GPU feature not enabled, falling back to CPU optimization");
            b_search_optimized(cursors, &bfwd, args.k, args.alpha, args.beta, true)
        }
    } else if args.optimized || config.use_simd || config.use_blas {
        eprintln!("Using optimized CPU implementation");
        b_search_optimized(cursors, &bfwd, args.k, args.alpha, args.beta, true)
    } else {
        eprintln!("Using original implementation");
        b_search(cursors, &bfwd, args.k, args.alpha, args.beta)
    };

    eprintln!("Exporting TREC run");
    // 6. Log results into TREC format
    print!("{}", to_trec(&q_ids, results, index.documents()));
    Ok(())
}
