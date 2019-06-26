use std::path::Path;

use crate::common::{stage2_config::Stage2Config, MigError};

// TODO: partition & untar balena to drive

pub(crate) fn partition(device: &Path, config: &Stage2Config) -> Result<(), MigError> {
    unimplemented!()
}
