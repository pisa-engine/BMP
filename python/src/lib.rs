use bmp::query::cursors_from_queries;
use bmp::query::MAX_TERM_WEIGHT;
use bmp::search::b_search_verbose;
use bmp::util::to_trec;
use bmp::CiffToBmp;
use pyo3::prelude::*;
use std::path::PathBuf;
use std::collections::HashMap;
use bmp::index::posting_list::PostingListIterator;

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

#[pyclass]
struct Searcher {
    index: bmp::index::inverted_index::Index,
    bfwd: bmp::index::forward_index::BlockForwardIndex,
}

#[pymethods]
impl Searcher {

    #[new]
    fn py_new(path: PathBuf) -> PyResult<Self> {
        let (index, bfwd) = bmp::index::from_file(path).expect("Index cannot be loaded.");
        Ok(Searcher {index: index, bfwd: bfwd})
    }

    fn search(
        &self,
        mut query: HashMap<String, u32>,
        k: usize,
        bsize: usize,
        alpha: f32,
        beta: f32,
    ) -> PyResult<(Vec<String>, Vec<f32>)> {
        let max_tok_weight = query.iter().map(|p| *p.1).max().unwrap();
        // let mut quant_query = HashMap<String, f32>;
        if max_tok_weight > MAX_TERM_WEIGHT as u32 {
            let scale: f32 = MAX_TERM_WEIGHT as f32 / max_tok_weight as f32;
            for value in query.values_mut() {
                *value = (*value as f32 * scale).ceil() as u32;
            }
        }
        let cursors: Vec<PostingListIterator> = query
            .iter()
            .flat_map(|(token, freq)| self.index.get_cursor(token, *freq))
            .collect();
        let wrapped_cursors = vec![cursors; 1];
        let mut results = b_search_verbose(wrapped_cursors, &self.bfwd, k, bsize, alpha, beta, false);
        let doc_lexicon = self.index.documents();
        let mut docnos: Vec<String> = Vec::new();
        let mut scores: Vec<f32> = Vec::new();
        for r in results[0].to_sorted_vec().iter() {
            docnos.push(doc_lexicon[r.doc_id.0 as usize].clone());
            scores.push(r.score.into());
        }
        Ok((docnos, scores))
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
    let results = b_search_verbose(cursors, &bfwd, k, bsize, alpha, beta, true);

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
    m.add_class::<Searcher>()?;
    Ok(())
}
