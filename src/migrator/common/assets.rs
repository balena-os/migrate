use std::fs::OpenOptions;
use std::path::Path;

use crate::common::{MigErrCtx, MigErrorKind};
use crate::{
    common::{path_append, MigError},
    defs::{MIG_INITRD_NAME, MIG_KERNEL_NAME},
};
use failure::ResultExt;
use std::io::Write;

pub struct Assets {
    pub asset_type: String,
    pub kernel: &'static [u8],
    pub initramfs: &'static [u8],
    // pub stage2: &'static [u8],
    pub dtbs: Vec<(String, &'static [u8])>,
}

impl Assets {
    fn write_asset<P: AsRef<Path>>(
        work_dir: P,
        filename: &str,
        bytes: &[u8],
    ) -> Result<(), MigError> {
        let target_path = path_append(work_dir.as_ref(), filename);
        let mut target_file = OpenOptions::new()
            .create(true)
            .read(false)
            .open(&target_path)
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to open file for writing: '{}'",
                    target_path.display()
                ),
            ))?;

        let bytes_written = target_file.write(bytes).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to write file: '{}'", target_path.display()),
        ))?;

        assert_eq!(bytes_written, bytes.len());

        Ok(())
    }

    pub fn write_to<P: AsRef<Path>>(&self, work_dir: P) -> Result<(), MigError> {
        let work_dir = work_dir.as_ref();

        Assets::write_asset(work_dir, MIG_KERNEL_NAME, self.kernel)?;
        Assets::write_asset(work_dir, MIG_INITRD_NAME, self.initramfs)?;
        // Assets::write_asset(work_dir, MIG_STAGE2_NAME, self.stage2)?;

        for (name, bytes) in &self.dtbs {
            Assets::write_asset(work_dir, &name, bytes)?;
        }

        Ok(())
    }
}
