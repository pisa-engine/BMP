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
fn search(a: usize, b: usize) -> PyResult<String> {
    Ok("Method not implemented yet")
}

/// A Python module implemented in Rust. The name of this function must match
/// the `lib.name` setting in the `Cargo.toml`, else Python will not be able to
/// import the module.
#[pymodule]
fn bmpy(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(ciff2bmp, m)?)?;
    m.add_function(wrap_pyfunction!(search, m)?)?;
    Ok(())
}
