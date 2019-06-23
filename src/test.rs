use std::env;
use nix::{ unistd, sys::stat, };
use std::fs::{OpenOptions, remove_file};
use std::io::{self, Read, Write};
use flate2::read::GzDecoder;
use log::{info, error};
use std::thread;
use std::process::{Command};
use mod_logger::{Logger, Level};

const OUTPUT_NAME: &str = "/tmp/test_sfdisk.img";

const MAX_WRITE: Option<usize> = Some(700 * 1024 * 1024);
const BUFFER_SIZE: usize  = 1024 *1024; // 1Mb




fn get_part_table(input_file: &str) {

}


fn write_to_pipe(input_file: &str,output_name: &str, max: Option<usize>) -> Result<usize,io::Error> {

    let mut output = if let Some(_usize) = max {
        match OpenOptions::new()
                .write(true)
                .create(true)
                .open(output_name) {
            Ok(file) => file,
            Err(why) => {
                error!("Failed to open output '{}', error: {:?}", output_name, why);
                return Err(why);
            }
        }
    } else {
        match OpenOptions::new()
            .write(true)
            .create(false)
            .open(output_name) {
            Ok(file) => file,
            Err(why) => {
                error!("Failed to open output '{}', error: {:?}", output_name, why);
                return Err(why);
            }
        }
    };

    let mut decoder = GzDecoder::new(
        match OpenOptions::new()
            .read(true)
            .create(false)
            .open(input_file) {
            Ok(file) => file,
            Err(why) => {
                error!("Failed to open image file '{}', error: {:?}", input_file, why);
                return Err(why);
            }
        });

    let buffer: &mut [u8] = &mut [0;BUFFER_SIZE];
    let mut bytes_written: usize = 0;

    loop {
        let bytes_read = match decoder.read(buffer) {
            Ok(bytes_read) => bytes_read,
            Err(why) => {
                error!("Failed to read from input '{}', error: {:?}", input_file, why);
                return Err(why);
            }
        };
        if bytes_read == 0 {
            return Ok(bytes_written);
        }

        let written = match output.write(&buffer[0..bytes_read]) {
            Ok(written) => written,
            Err(why) => {
                error!("Failed to read from input '{}', error: {:?}", input_file, why);
                return Err(why);
            }
        };

        if written != bytes_read {
            error!("Differing values of bytes written & bytes read {} != {}", written, bytes_read);
        }

        bytes_written += written;

        if let Some(max) = max {
            if bytes_written >= max {
                return  Ok(bytes_written);
            }
        }
    }
}


fn main() {
    let _res = Logger::set_default_level(&Level::Debug);
    println!("test entered");

    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {

        let input_file = args[1].clone();

        info!("input_file: '{}'", input_file);


        if let Some(_max) = MAX_WRITE {
            let _res = write_to_pipe(input_file.as_str(), OUTPUT_NAME, MAX_WRITE);
        } else {
            if let Err(why) = unistd::mkfifo(OUTPUT_NAME, stat::Mode::S_IRWXU) {
                error!("Failed to create fifo: {}, error: {:?}", OUTPUT_NAME, why);
                return;
            }

            let _res = thread::spawn(move||  write_to_pipe(input_file.as_str(), OUTPUT_NAME, None)  );
        }

        let args: &[&str] = &["--dump", OUTPUT_NAME];

        let output = match Command::new("sfdisk")
            .args(args)
            .output() {
            Ok(output) => output,
            Err(why) => {
                error!("Failed to start sfdisk, error: {:?}", why);
                let _res = remove_file(OUTPUT_NAME);
                return;
            }
        };

        if output.status.success() {
            println!("result: {}", String::from_utf8_lossy(&output.stdout));
        } else {
            error!("got result code from sfdisk: {:?}", output.status.code());
            error!("sfdisk stderr: {}", String::from_utf8_lossy(&output.stderr));
        }
    } else {
        println!("Usage: <test input file>");
    }


    println!("test done");
}
