#[cfg(target_os = "linux")]
fn main() {
    use balena_migrate::{common::MigErrorKind, extract};
    if let Err(error) = extract() {
        match error.kind() {
            MigErrorKind::Displayed => {
                println!("balena-extract failed with an error, see messages above");
            }
            _ => {
                println!("balena-extract failed with an error: {}", error);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn main() {
    println!("This program is only meant to be run on linux");
}
