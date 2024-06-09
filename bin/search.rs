use anyhow::Result;

use bmp::query::cursors_from_queries;
use bmp::search::b_search;
use bmp::util::to_trec;
use std::path::PathBuf;
use structopt::StructOpt;

// Function to perform the search for each query and return the results

#[derive(Debug, StructOpt)]
#[structopt(name = "search", about = "Search an index and produce a TREC output")]

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
    let results = b_search(cursors, &bfwd, args.k, args.alpha, args.beta);

    eprintln!("Exporting TREC run");
    // 4. Log results into TREC format
    print!("{}", to_trec(&q_ids, results, index.documents()));
    Ok(())
}
