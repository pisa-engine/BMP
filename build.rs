extern crate protobuf_codegen_pure;

use std::env;
use std::fs;

fn main() {
    // Create proto output directory if it doesn't exist
    fs::create_dir_all("src/proto").expect("Failed to create proto directory");

    // Generate protobuf files
    protobuf_codegen_pure::Codegen::new()
        .out_dir("src/proto")
        .inputs(&["proto/common-index-format-v1.proto"])
        .include("proto")
        .run()
        .expect("protoc");

    // Ensure Accelerate framework is linked on macOS
    if env::var("CARGO_CFG_TARGET_OS").unwrap() == "macos" {
        println!("cargo:rustc-link-lib=framework=Accelerate");
    }
}
