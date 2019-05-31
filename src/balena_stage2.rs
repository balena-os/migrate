#[cfg(target_os = "linux")]
fn main() {
    use balena_migrate::stage2;
    if let Err(error) = stage2() {
        println!("got error from stage2: {}", error);
    }
}

#[cfg(target_os = "windows")]
fn main() {
    println!("This program is only meant to be run on linux");
}
