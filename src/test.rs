use flate2::{write::GzEncoder, Compression};
use std::fs::File;
use std::path::PathBuf;
use tar::Builder;

fn main() {
    let encoder = GzEncoder::new(File::create("test.tar.gz").unwrap(), Compression::default());
    let mut tar_builder = Builder::new(encoder);

    tar_builder
        .append_path_with_name("src/test.rs", "save/toast.rs")
        .unwrap();

    tar_builder.finish().unwrap();

    // encoder.finish().unwrap();
}
