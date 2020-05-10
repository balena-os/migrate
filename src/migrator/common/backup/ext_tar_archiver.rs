use failure::ResultExt;
use log::{debug, error, warn};
use std::fs::{create_dir_all, remove_dir_all};
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
use std::os::unix::fs::symlink;

use crate::{
    common::{
        backup::archiver::Archiver, call, dir_exists, path_append, MigErrCtx, MigError,
        MigErrorKind,
    },
    defs::BACKUP_FILE,
    linux::linux_defs::{MKTEMP_CMD, TAR_CMD},
};

// use external tar / gzip for archiving
// strategy is to link  (ln -s ) all files / directories to a temporary directory
// and tar/gizip that directory on finish
#[cfg(target_os = "linux")]
pub(crate) struct ExtTarArchiver {
    tmp_dir: PathBuf,
    archive: PathBuf,
}

#[cfg(target_os = "linux")]
impl ExtTarArchiver {
    pub fn new<P: AsRef<Path>>(file: P) -> Result<ExtTarArchiver, MigError> {
        let cmd_res = call(MKTEMP_CMD, &["-d"], true).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            "failed to create temporary directory for backup",
        ))?;

        if !cmd_res.status.success() {
            error!("Failed to create temporary directory");
            return Err(MigError::displayed());
        }

        Ok(ExtTarArchiver {
            tmp_dir: PathBuf::from(cmd_res.stdout),
            archive: PathBuf::from(file.as_ref()),
        })
    }
}

#[cfg(target_os = "linux")]
impl Archiver for ExtTarArchiver {
    fn add_file(&mut self, target: &Path, source: &Path) -> Result<(), MigError> {
        debug!(
            "ExtTarArchiver::add_file: '{}' , '{}'",
            target.display(),
            source.display()
        );
        if let Some(parent_dir) = target.parent() {
            let parent_dir = path_append(&self.tmp_dir, parent_dir);
            if !dir_exists(&parent_dir).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to access directory '{}'", parent_dir.display()),
            ))? {
                debug!(
                    "ExtTarArchiver::add_file: create directory '{}'",
                    parent_dir.display()
                );
                create_dir_all(&parent_dir).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("Failed to create directory '{}'", parent_dir.display()),
                ))?;
            }
        }

        let lnk_target = path_append(&self.tmp_dir, &target);

        debug!(
            "ExtTarArchiver::add_file: link '{}' to '{}'",
            source.display(),
            lnk_target.display()
        );

        symlink(source, &lnk_target).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Failed to link '{}' to '{}'",
                source.display(),
                lnk_target.display()
            ),
        ))?;
        Ok(())
    }

    fn finish(&mut self) -> Result<(), MigError> {
        let cmd_res = call(
            TAR_CMD,
            &[
                "-h",
                "-czf",
                BACKUP_FILE,
                "-C",
                &*self.tmp_dir.to_string_lossy(),
                ".",
            ],
            true,
        )
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Failed to create backup archive '{}'",
                self.archive.display()
            ),
        ))?;

        if !cmd_res.status.success() {
            error!(
                "Failed to create archive in '{}', message: '{}'",
                self.archive.display(),
                cmd_res.stderr
            );
            return Err(MigError::displayed());
        }

        if let Err(why) = remove_dir_all(&self.tmp_dir) {
            warn!(
                "Failed to delete temporary directory '{}' error: {:?}",
                self.tmp_dir.display(),
                why
            );
        }

        Ok(())
    }
}
