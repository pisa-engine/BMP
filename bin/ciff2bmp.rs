use bmp::CiffToBmp;
use std::path::PathBuf;
use structopt::StructOpt;

/// Struct for command-line arguments
#[derive(Debug, StructOpt)]
#[structopt(
    name = "ciff2bmp",
    about = "Generates a BMP index from a Common Index Format [v1]"
)]
struct Args {
    #[structopt(short, long, help = "Path to ciff export file")]
    ciff_file: PathBuf,
    #[structopt(short, long, help = "Output filename")]
    output: PathBuf,
    #[structopt(short, long, help = "Block size")]
    bsize: usize,
    #[structopt(short, long, help = "Compress range data")]
    compress_range: bool,
}

fn main() {
    // Parse command-line arguments
    let args = Args::from_args();

    // Create a CiffToBmp converter with default settings
    let mut converter = CiffToBmp::default();

    // Set the input and output paths for the converter
    converter
        .input_path(args.ciff_file)
        .output_path(args.output)
        .compress_range(args.compress_range)
        .bsize(args.bsize);

    // Convert the Ciff file to BMP format
    if let Err(error) = converter.to_bmp() {
        // Handle and print any errors
        eprintln!("ERROR: {}", error);
        std::process::exit(1);
    }
}
