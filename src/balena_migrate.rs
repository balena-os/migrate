// use crate::balena_migrate::migrator;
use balena_migrate::migrate;

fn main() {
    // TODO: display error
    if let Err(error) = migrate() {
        println!("got error from migrator: {}", error);
    }
}
