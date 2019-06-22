use std::path::{Path};

use crate::{
    common::{
        MigError, MigErrorKind,
        stage2_config::{Stage2Config}
    },
};

// TODO: partition & untar balena to drive

pub(crate) fn partition(device : &Path, config: &Stage2Config) -> Result<(),MigError> {
    unimplemented!()
}