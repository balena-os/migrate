use std::path::PathBuf;
use std::process::{Command, Stdio};

fn main() {
    let gzip_path = PathBuf::from("work/test/test.gz");

    println!("invoking gzip");

    let cmd1 = Command::new("gzip")
        .args(&["-d", "-c", &gzip_path.to_string_lossy()])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    /*
        let stdout = cmd1.stdout.as_mut().unwrap();
        let mut buf : [u8;1024] = [0; 1024];
        loop {
            let read = stdout.read(&mut buf).unwrap();

            println!("read {} bytes", read);
            if read == 0 {
                break;
            }
        }
    */

    if let Some(cmd1_stdout) = cmd1.stdout {
        let cmd_res = Command::new("cat").stdin(cmd1_stdout).output().unwrap();
        println!("dd command result: {:?}", cmd_res);
    } else {
        println!("no stdout found");
    }
}
