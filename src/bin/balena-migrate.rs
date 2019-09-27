// use crate::balena_migrate::migrator;
use balena_migrate::{common::MigErrorKind, migrate};

fn main() {
    // TODO: display error
    if let Err(error) = migrate() {
        match error.kind() {
            MigErrorKind::Displayed => {
                println!("balena-migrate failed with an error, see messages above");
            }
            _ => {
                println!("balena-migrate failed with an error: {}", error);
            }
        }
    }
}
