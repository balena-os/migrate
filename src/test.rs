use mod_logger::{Level, Logger};
use std::fs::File;
use std::path::PathBuf;
use tar::Builder;

use balena_migrate::test;

fn main() {
    println!("test entered");
    test().unwrap();
    // encoder.finish().unwrap();
    println!("test done");
}
