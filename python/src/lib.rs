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
        query: HashMap<String, f32>,
        k: usize,
        alpha: f32,
        beta: f32,
    ) -> PyResult<(Vec<String>, Vec<f32>)> {
        let max_tok_weight = query.iter().map(|p| *p.1).filter(|&value| !value.is_nan()).max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
        let mut quant_query: HashMap<String, u32> = HashMap::new();
        
        let scale: f32 = MAX_TERM_WEIGHT as f32 / max_tok_weight;
        for (key, value) in &query {
            quant_query.insert(key.clone(), (value * scale).ceil() as u32);
        }
        let cursors: Vec<PostingListIterator> = quant_query
            .iter()
            .flat_map(|(token, freq)| self.index.get_cursor(token, *freq))
            .collect();
        let wrapped_cursors = vec![cursors; 1];
        let mut results = b_search_verbose(wrapped_cursors, &self.bfwd, k, alpha, beta, false);
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
    alpha: f32,
    beta: f32,
) -> PyResult<String> {
    eprintln!("Loading the index");
    let (index, bfwd) = bmp::index::from_file(index).expect("Index cannot be loaded.");

    // 2. Load the queries
    eprintln!("Loading the queries");
    let (q_ids, cursors) = cursors_from_queries(queries, &index);

    eprintln!("Performing query processing");
    let results = b_search_verbose(cursors, &bfwd, k, alpha, beta, true);

    eprintln!("Exporting TREC run");
    // 4. Log results into TREC format
    Ok(to_trec(&q_ids, results, index.documents()))
}

#[pyclass]
struct Indexer {
    path: PathBuf,
    bsize: usize,
    compress_range: bool,
    inv_builder: bmp::index::inverted_index::IndexBuilder,
    fwd_builder: bmp::index::forward_index::ForwardIndexBuilder,
}

#[pymethods]
impl Indexer {

    #[new]
    fn py_new(path: PathBuf, bsize: usize, compress_range: bool) -> PyResult<Self> {
        Ok(Indexer {
            path: path,
            bsize: bsize,
            compress_range: compress_range,
            inv_builder: bmp::index::inverted_index::IndexBuilder::new(0, bsize),
            fwd_builder: bmp::index::forward_index::ForwardIndexBuilder::new(0),
        })
    }

    fn add_document(
        &mut self,
        doc_id: String,
        vector: Vec<(u32, u32)>,
    ) -> PyResult<()> {
      self.inv_builder.insert_document(&doc_id);
      self.fwd_builder.insert_document(vector);
      Ok(())
    }

    fn add_term(
        &mut self,
        term: String,
        postings: Vec<(u32, u32)>,
    ) -> PyResult<()> {
      self.inv_builder.insert_term(&term, postings);
      Ok(())
    }

    fn finish(
        &mut self,
    ) -> PyResult<()> {
        let builder = std::mem::replace(&mut self.inv_builder, bmp::index::inverted_index::IndexBuilder::new(0, 0));
        let inverted_index = builder.build(self.compress_range);
        let forward_index = self.fwd_builder.build();
        let b_forward_index = bmp::index::forward_index::fwd2bfwd(&forward_index, self.bsize);
        let file = std::fs::File::create(self.path.clone()).expect("Failed to create file");
        let writer = std::io::BufWriter::new(file);
        // Serialize the index directly into a file using bincode
        bincode::serialize_into(writer, &(&inverted_index, &b_forward_index))
            .expect("Failed to serialize");
        Ok(())
    }
}

/// A Python module implemented in Rust. The name of this function must match
/// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
/// import the module.
#[pymodule]
fn _bmp(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(ciff2bmp, m)?)?;
    m.add_function(wrap_pyfunction!(search, m)?)?;
    m.add_class::<Searcher>()?;
    m.add_class::<Indexer>()?;
    Ok(())
}
