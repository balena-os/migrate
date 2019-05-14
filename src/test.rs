use std::path::PathBuf;
use std::fs::{File};
use tar::{Builder};
use flate2::{Compression, write::GzEncoder};


fn main() {
    let encoder = GzEncoder::new(File::create("test.tar.gz").unwrap(), Compression::default());
    let mut tar_builder = Builder::new(encoder);

    tar_builder.append_path_with_name("src/test.rs", "save/toast.rs").unwrap();

    tar_builder.finish().unwrap();

    // encoder.finish().unwrap();

}
