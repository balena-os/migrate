// executable wrapper for balena-migrate

use balena_migrate::{
    common::{assets::Assets, MigErrorKind},
    migrate,
};

fn init_assets() -> Assets {
    Assets {
        version: include_bytes!("../../build_assets/tmp/version.yml"),
        data: include_bytes!("../../build_assets/assets.tgz"),
    }
}

fn main() {
    if let Err(error) = migrate(init_assets()) {
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
