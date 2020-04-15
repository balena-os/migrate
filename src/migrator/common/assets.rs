use std::path::Path;

use serde::Deserialize;
use serde_yaml;

use crate::common::{MigErrCtx, MigError, MigErrorKind};
use failure::ResultExt;
use flate2::read::GzDecoder;
use tar::Archive;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct AssetVersion {
    pub device: String,
    pub kernel: String,
    pub balena: String,
}

pub struct Assets {
    pub version: &'static [u8],
    pub data: &'static [u8],
}

impl Assets {
    pub fn get_version(&self) -> Result<AssetVersion, MigError> {
        let version: AssetVersion =
            serde_yaml::from_slice(self.version).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to deserialize version info for assets"),
            ))?;

        Ok(version)
    }

    pub fn write_to<P: AsRef<Path>>(&self, work_dir: P) -> Result<(), MigError> {
        // TODO: untar to target dir
        Archive::new(GzDecoder::new(self.data))
            .unpack(work_dir)
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                "Failed to unpack assets archive contents",
            ))?;

        Ok(())
    }
}
