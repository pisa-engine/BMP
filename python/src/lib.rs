use bmp::query::cursors_from_queries;
use bmp::search::b_search;
use bmp::util::to_trec;
use bmp::CiffToBmp;
use pyo3::prelude::*;
use std::path::PathBuf;

#[pyfunction]
fn ciff2bmp(ciff_file: PathBuf, output: PathBuf, bsize: usize, compress_range: bool) {
    let mut converter = CiffToBmp::default();
    converter
        .input_path(ciff_file)
        .output_path(output)
        .compress_range(compress_range)
        .bsize(bsize);
    if let Err(error) = converter.to_bmp() {
        eprintln!("ERROR: {}", error);
        std::process::exit(1);
    }
}

#[pyfunction]
fn search(
    index: PathBuf,
    queries: PathBuf,
    k: usize,
    bsize: usize,
    alpha: f32,
    beta: f32,
) -> PyResult<String> {
    eprintln!("Loading the index");
    let (index, bfwd) = bmp::index::from_file(index).expect("Index cannot be loaded.");

    // 2. Load the queries
    eprintln!("Loading the queries");
    let (q_ids, cursors) = cursors_from_queries(queries, &index);

    eprintln!("Performing query processing");
    let results = b_search(cursors, &bfwd, k, bsize, alpha, beta);

    eprintln!("Exporting TREC run");
    // 4. Log results into TREC format
    Ok(to_trec(&q_ids, results, index.documents()))
}

/// A Python module implemented in Rust. The name of this function must match
/// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
/// import the module.
#[pymodule]
fn bmpy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(ciff2bmp, m)?)?;
    m.add_function(wrap_pyfunction!(search, m)?)?;
    Ok(())
}
