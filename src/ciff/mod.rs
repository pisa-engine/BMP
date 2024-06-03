use anyhow::{anyhow, Context};
use indicatif::{ProgressBar, ProgressStyle};
use num_traits::ToPrimitive;
use protobuf::CodedInputStream;
use std::fmt;
use std::fs::File;
use std::io::BufWriter;
use std::io::{Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::index::forward_index::ForwardIndexBuilder;
use crate::index::inverted_index::IndexBuilder;

pub use crate::proto::{DocRecord, Posting, PostingsList};

type Result<T> = anyhow::Result<T>;

/// Wraps [`proto::Header`] and additionally provides some important counts that are already cast
/// to an unsigned type.
#[derive(PartialEq, Clone, Default)]
struct Header {
    num_postings_lists: u32,
    num_documents: u32,
    /// Used for printing.
    protobuf_header: crate::proto::Header,
}

impl Header {
    /// Reads the protobuf header, and converts to a proper-typed header to fail fast if the protobuf
    /// header contains any negative values.
    ///
    /// # Errors
    ///
    /// Returns an error if the protobuf header contains negative counts.
    fn from_stream(input: &mut CodedInputStream<'_>) -> Result<Self> {
        let header = input.read_message::<crate::proto::Header>()?;
        let num_documents = u32::try_from(header.get_num_docs())
            .context("Number of documents must be non-negative.")?;
        let num_postings_lists = u32::try_from(header.get_num_postings_lists())
            .context("Number of documents must be non-negative.")?;
        Ok(Self {
            protobuf_header: header,
            num_documents,
            num_postings_lists,
        })
    }
}

impl fmt::Display for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.protobuf_header)
    }
}

const DEFAULT_PROGRESS_TEMPLATE: &str =
    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {count}/{total} ({eta})";

/// Returns default progress style.
fn pb_style() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template(DEFAULT_PROGRESS_TEMPLATE)
        .progress_chars("=> ")
}

/// CIFF to BMP converter.
#[derive(Debug, Default, Clone)]
pub struct CiffToBmp {
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    bsize: Option<usize>,
    compress_range: bool,
}

impl CiffToBmp {
    /// Sets the CIFF path. Required.
    pub fn input_path<P: Into<PathBuf>>(&mut self, path: P) -> &mut Self {
        self.input = Some(path.into());
        self
    }

    /// Sets BMP (uncompressed) inverted index path. Required.
    pub fn output_path<P: Into<PathBuf>>(&mut self, path: P) -> &mut Self {
        self.output = Some(path.into());
        self
    }

    pub fn bsize(&mut self, bsize: usize) -> &mut Self {
        self.bsize = Some(bsize);
        self
    }
    pub fn compress_range(&mut self, compress_range: bool) -> &mut Self {
        self.compress_range = compress_range;
        self
    }
    /// Builds a BMP index using the previously defined parameters.
    ///
    /// # Errors
    ///
    /// Error will be returned if:
    ///  - some required parameters are not defined,
    ///  - any I/O error occurs during reading input files or writing to the output file,
    ///  - any input file is in an incorrect format.
    pub fn to_bmp(&self) -> Result<()> {
        let input = self
            .input
            .as_ref()
            .ok_or_else(|| anyhow!("input path undefined"))?;
        let output = self
            .output
            .as_ref()
            .ok_or_else(|| anyhow!("input path undefined"))?;
        let bsize = self.bsize.ok_or_else(|| anyhow!("bsize undefined"))?;
        convert_to_bmp(input, output, bsize, self.compress_range)
    }
}

fn convert_to_bmp(input: &Path, output: &Path, bsize: usize, compress_range: bool) -> Result<()> {
    println!("{:?}", output);
    let mut ciff_reader =
        File::open(input).with_context(|| format!("Unable to open {}", input.display()))?;

    let mut builder: IndexBuilder;
    {
        let mut input = CodedInputStream::new(&mut ciff_reader);

        let header: Header = Header::from_stream(&mut input)?;
        println!("{}", header);

        builder = IndexBuilder::new(header.num_documents as usize, bsize);

        eprintln!("Processing postings");
        let progress = ProgressBar::new(u64::try_from(header.num_postings_lists)?);
        progress.set_style(pb_style());
        progress.set_draw_delta(10);

        for _ in 0..header.num_postings_lists {
            let list = input.read_message::<PostingsList>()?;
            let mut docid = 0;
            let postings: Vec<(u32, u32)> = list
                .get_postings()
                .iter()
                .map(|p| {
                    docid += u32::try_from(p.get_docid()).expect("Negative docID");
                    (
                        docid,
                        u32::try_from(p.get_tf()).expect("Negative frequency"),
                    )
                })
                .collect();
            builder.insert_term(list.term.as_str(), postings);
            progress.inc(1);
        }
        progress.finish();

        eprintln!("Processing document names");

        let progress = ProgressBar::new(u64::from(header.num_documents));
        progress.set_style(pb_style());
        progress.set_draw_delta(u64::from(header.num_documents) / 100);

        for docs_seen in 0..header.num_documents {
            let doc_record = input.read_message::<DocRecord>()?;

            let docid: u32 = doc_record
                .get_docid()
                .to_u32()
                .ok_or_else(|| anyhow!("Cannot cast docid to u32: {}", doc_record.get_docid()))?;

            let trecid = doc_record.get_collection_docid();
            if docid != docs_seen {
                anyhow::bail!("Document sizes must come in order");
            }
            builder.insert_document(trecid);
            progress.inc(1);
        }
        progress.finish();
    }
    let inverted_index = builder.build(compress_range);

    // Seek to the beginning of the file
    ciff_reader.seek(SeekFrom::Start(0))?;

    // Recreate the CodedInputStream with the file reader reset to the beginning
    let mut input = CodedInputStream::new(&mut ciff_reader);
    let header = Header::from_stream(&mut input)?;

    eprintln!("Building forward index");
    let progress = ProgressBar::new(header.num_postings_lists as u64);
    progress.set_style(pb_style());
    progress.set_draw_delta((header.num_postings_lists / 100) as u64);

    let mut fwd_builder = ForwardIndexBuilder::new(header.num_documents as usize);

    for term_id in 0..header.num_postings_lists {
        let list = input.read_message::<PostingsList>()?;
        let mut docid = 0;
        let posting_list: Vec<(u32, u32)> = list
            .get_postings()
            .iter()
            .map(|p| {
                docid += u32::try_from(p.get_docid()).expect("Negative docID");
                (
                    docid,
                    u32::try_from(p.get_tf()).expect("Negative frequency"),
                )
            })
            .collect();

        fwd_builder.insert_posting_list(term_id as u32, &posting_list);
        progress.inc(1);
    }
    // for (term_id, posting_list) in inverted_index.posting_lists().iter().enumerate() {
    //     fwd_builder.insert_posting_list(term_id as u32, posting_list);
    //     progress.inc(1);
    // }
    progress.finish();
    eprintln!("Converting to blocked forward index");

    let forward_index = fwd_builder.build();
    let b_forward_index = crate::index::forward_index::fwd2bfwd(&forward_index, bsize);
    eprintln!("block numbers: {}", b_forward_index.data.len());
    let mut tot = 0;
    let mut tot_avg_docs = 0.0;
    for (_, block) in b_forward_index.data.iter().enumerate() {
        tot += block.len();
        tot_avg_docs +=
            block.iter().map(|(_, v)| v.len()).sum::<usize>() as f32 / block.len() as f32;
    }
    eprintln!("avg terms per block: {}", tot / b_forward_index.data.len());
    eprintln!(
        "avg docs per term: {}",
        tot_avg_docs / b_forward_index.data.len() as f32
    );
    let file = File::create(output).expect("Failed to create file");
    let writer = BufWriter::new(file);
    // Serialize the index directly into a file using bincode
    bincode::serialize_into(writer, &(&inverted_index, &b_forward_index))
        .expect("Failed to serialize");

    Ok(())
}
