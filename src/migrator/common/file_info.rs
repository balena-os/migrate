use failure::ResultExt;
#[allow(unused_imports)]
use log::{debug, error, trace};
#[allow(unused_imports)]
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ******************************************************************
// Find location and size of file as absolute or relative to workdir
// make a guess on file contents / type and compare to expected value
// ******************************************************************

// TODO: make hash_info optional again
// creating a digest in stage1 for check in stage2 does not mae a lot of sense.

use crate::common::os_api::{OSApi, OSApiImpl};
use crate::common::{
    file_digest::{get_default_digest, HashInfo},
    //file_digest::check_digest
    file_exists,
    MigErrCtx,
    MigError,
    MigErrorKind,
};

// #[cfg(target_os = "linux")]

#[derive(Debug, Clone)]
pub(crate) struct FileInfo {
    pub path: PathBuf,
    pub rel_path: Option<PathBuf>,
    pub size: u64,
    pub hash_info: HashInfo,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct RelFileInfo {
    pub rel_path: PathBuf,
    pub size: u64,
    pub hash_info: HashInfo,
}

// TODO: make this detect file formats used by migrate, eg: kernel, initramfs, json file, disk image

impl FileInfo {
    pub fn new<P1: AsRef<Path>, P2: AsRef<Path>>(
        file: P1,
        work_dir: P2,
    ) -> Result<Option<FileInfo>, MigError> {
        let os_api = OSApiImpl::new()?;
        let file_path = file.as_ref();
        let work_path = os_api.canonicalize(work_dir.as_ref())?;

        trace!(
            "FileInfo::new: entered with file: '{}', work_dir: '{}'",
            file_path.display(),
            work_path.display()
        );

        // figure out if this a path relative to work_dir rather than absolute or relative to current dir

        let checked_path = if file_exists(file_path) {
            PathBuf::from(file_path)
        } else if !file_path.is_absolute() {
            let search_path = work_path.join(file_path);
            if search_path.exists() {
                search_path
            } else {
                // tried to build path using workdir, but nothing found
                return Ok(None);
            }
        } else {
            // Absolute path was not found, no hope
            return Ok(None);
        };

        trace!("working with path: '{}'", checked_path.display());

        let abs_path =
            OSApiImpl::new()?
                .canonicalize(&checked_path)
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("Failed to canonicalize path '{}'", checked_path.display()),
                ))?;

        trace!("working with abs_path: '{}'", abs_path.display());

        let metadata = abs_path.metadata().context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("failed to retrieve metadata for path {:?}", abs_path),
        ))?;

        trace!("got metadata for: '{}'", abs_path.display());

        let rel_path = match abs_path.strip_prefix(work_path) {
            Ok(rel_path) => Some(PathBuf::from(rel_path)),
            Err(_why) => None,
        };

        trace!(
            "got relative path for: '{}': '{:?}'",
            abs_path.display(),
            rel_path.as_ref()
        );

        debug!("done creating FileInfo for '{}'", file_path.display());
        let hash_info = get_default_digest(&abs_path)?;
        Ok(Some(FileInfo {
            path: abs_path,
            rel_path,
            size: metadata.len(),
            hash_info,
        }))
    }

    pub fn to_rel_fileinfo(&self) -> Result<RelFileInfo, MigError> {
        if let Some(ref rel_path) = self.rel_path {
            Ok(RelFileInfo {
                rel_path: rel_path.clone(),
                size: self.size,
                hash_info: self.hash_info.clone(),
            })
        } else {
            error!(
                "The file '{}' was not found in the working directory",
                self.path.display()
            );
            Err(MigError::displayed())
        }
    }
}
