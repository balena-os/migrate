// use crate::balena_migrate::migrator;
use balena_migrate::{common::MigErrorKind, migrate};

fn main() {
    // TODO: display error
    if let Err(error) = migrate() {
        match error.kind() {
            MigErrorKind::Displayed => (),
            _ => {
                println!("got error from migrator: {}", error);
            }
        }
    }
}
