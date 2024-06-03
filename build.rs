//! Here, we generate Rust code from a proto file before project compilation.
use std::env;
use std::fs::{read_to_string, File};
use std::io::{BufWriter, Write};
use std::path::Path;

fn main() {
    let out_dir_env = env::var_os("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir_env);
    protobuf_codegen_pure::Codegen::new()
        .out_dir(out_dir)
        .inputs(["proto/common-index-format-v1.proto"])
        .include("proto")
        .run()
        .expect("Codegen failed.");
    let path = out_dir.join("common_index_format_v1.rs");
    let code = read_to_string(&path).expect("Failed to read generated file");
    let mut writer = BufWriter::new(File::create(path).unwrap());
    for line in code.lines() {
        if !line.contains("//!") && !line.contains("#!") {
            writer
                .write_all(line.as_bytes())
                .expect("Failed to write to generated file");
            writer
                .write_all(&[b'\n'])
                .expect("Failed to write to generated file");
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
}
