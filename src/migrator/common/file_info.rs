use failure::ResultExt;
#[cfg(target_os = "linux")]
use lazy_static::lazy_static;
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

use crate::common::{
    config::balena_config::FileRef,
    file_digest::{check_digest, get_default_digest, HashInfo},
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
    pub fn new<P: AsRef<Path>>(
        file_ref: &FileRef,
        work_dir: P,
    ) -> Result<Option<FileInfo>, MigError> {
        let file_path = &file_ref.path;
        let work_path = work_dir.as_ref();
        trace!(
            "FileInfo::new: entered with file: '{}', work_dir: '{}'",
            file_path.display(),
            work_path.display()
        );

        // figure out if this a path relative to work_dir rather than absolute or relative to current dir

        let checked_path = if file_exists(file_path) {
            PathBuf::from(file_path)
        } else {
            if !file_path.is_absolute() {
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
            }
        };

        let abs_path = checked_path.canonicalize().context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to canonicalize path '{}'", checked_path.display()),
        ))?;
        let metadata = abs_path.metadata().context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("failed to retrieve metadata for path {:?}", abs_path),
        ))?;

        let rel_path = match abs_path.strip_prefix(work_path) {
            Ok(rel_path) => Some(PathBuf::from(rel_path)),
            Err(_why) => None,
        };

        let hash_info = if let Some(ref hash_info) = file_ref.hash {
            if !check_digest(&file_ref.path, hash_info)? {
                error!(
                    "Failed to check file digest for file '{}': {:?}",
                    file_ref.path.display(),
                    hash_info
                );
                return Err(MigError::displayed());
            } else {
                hash_info.clone()
            }
        } else {
            debug!("Created digest for file: '{}'", file_ref.path.display());
            get_default_digest(&file_ref.path)?
        };

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
            return Err(MigError::displayed());
        }
    }
}
