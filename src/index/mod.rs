pub mod forward_index;
pub mod inverted_index;
pub mod posting_list;

use anyhow::Result;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

pub fn from_file<P: Into<PathBuf>>(
    index_path: P,
) -> Result<(inverted_index::Index, forward_index::BlockForwardIndex)> {
    let file = File::open(index_path.into())?;
    let reader = BufReader::new(file);
    Ok(bincode::deserialize_from(reader)?)
}
