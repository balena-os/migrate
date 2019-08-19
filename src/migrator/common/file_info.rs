use failure::ResultExt;
#[cfg(target_os = "linux")]
use lazy_static::lazy_static;
use log::{debug, error, trace};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// ******************************************************************
// Find location and size of file as absolute or relative to workdir
// make a guess on file contents / type and conpare to expected value
// ******************************************************************

// file on ubuntu-14.04 reports x86 boot sector for image and kernel files

const OS_IMG_FTYPE_REGEX: &str = r#"^(DOS/MBR boot sector|x86 boot sector)$"#;
const GZIP_OS_IMG_FTYPE_REGEX: &str =
    r#"^(DOS/MBR boot sector|x86 boot sector).*\(gzip compressed data.*\)$"#;

const INITRD_FTYPE_REGEX: &str = r#"^ASCII cpio archive.*\(gzip compressed data.*\)$"#;
const OS_CFG_FTYPE_REGEX: &str = r#"^ASCII text.*$"#;
const KERNEL_AMD64_FTYPE_REGEX: &str =
    r#"^(Linux kernel x86 boot executable bzImage|x86 boot sector).*$"#;
const KERNEL_ARMHF_FTYPE_REGEX: &str = r#"^Linux kernel ARM boot executable zImage.*$"#;
const KERNEL_I386_FTYPE_REGEX: &str = r#"^Linux kernel i386 boot executable bzImage.*$"#;
const TEXT_FTYPE_REGEX: &str = r#"^ASCII text.*$"#;
const DTB_FTYPE_REGEX: &str = r#"^(Device Tree Blob|data).*$"#;

const GZIP_TAR_FTYPE_REGEX: &str = r#"^(POSIX tar archive \(GNU\)).*\(gzip compressed data.*\)$"#;

use crate::common::{
    file_exists,
    MigErrCtx,
    MigError,
    MigErrorKind,
    //file_digest::check_digest
};

use crate::common::config::balena_config::FileRef;
use crate::common::file_digest::{check_digest, get_default_digest, HashInfo};
#[cfg(target_os = "linux")]
use crate::linux::{EnsuredCmds, FILE_CMD};

#[derive(Debug, Clone)]
pub(crate) enum FileType {
    GZipOSImage,
    OSImage,
    KernelAMD64,
    KernelARMHF,
    KernelI386,
    InitRD,
    Json,
    Text,
    DTB,
    GZipTar,
}

impl FileType {
    pub fn get_descr(&self) -> &str {
        match self {
            FileType::GZipOSImage => "gzipped balena OS image",
            FileType::OSImage => "balena OS image",
            FileType::KernelAMD64 => "balena migrate kernel image for AMD64",
            FileType::KernelARMHF => "balena migrate kernel image for ARMHF",
            FileType::KernelI386 => "balena migrate kernel image for I386",
            FileType::InitRD => "balena migrate initramfs",
            FileType::DTB => "Device Tree Blob",
            FileType::Json => "balena config.json file",
            FileType::Text => "Text file",
            FileType::GZipTar => "Gzipped Tar file",
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct FileInfo {
    pub path: PathBuf,
    pub rel_path: Option<PathBuf>,
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

        let abs_path = checked_path.canonicalize().unwrap();
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

    #[cfg(target_os = "linux")]
    pub fn expect_type(&self, cmds: &EnsuredCmds, ftype: &FileType) -> Result<(), MigError> {
        if !self.is_type(cmds, ftype)? {
            let message = format!(
                "Could not determine expected file type '{}' for file '{}'",
                ftype.get_descr(),
                self.path.display()
            );
            error!("{}", message);
            Err(MigError::from_remark(MigErrorKind::InvParam, &message))
        } else {
            Ok(())
        }
    }

    #[cfg(target_os = "linux")]
    pub fn is_type(&self, cmds: &EnsuredCmds, ftype: &FileType) -> Result<bool, MigError> {
        let path_str = self.path.to_string_lossy();
        let args: Vec<&str> = vec!["-bz", &path_str];

        let cmd_res = cmds.call(FILE_CMD, &args, true)?;
        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "new: failed determine type for file {}",
                    self.path.display()
                ),
            ));
        }

        lazy_static! {
            static ref OS_IMG_FTYPE_RE: Regex = Regex::new(OS_IMG_FTYPE_REGEX).unwrap();
            static ref GZIP_OS_IMG_FTYPE_RE: Regex = Regex::new(GZIP_OS_IMG_FTYPE_REGEX).unwrap();
            static ref INITRD_FTYPE_RE: Regex = Regex::new(INITRD_FTYPE_REGEX).unwrap();
            static ref OS_CFG_FTYPE_RE: Regex = Regex::new(OS_CFG_FTYPE_REGEX).unwrap();
            static ref TEXT_FTYPE_RE: Regex = Regex::new(TEXT_FTYPE_REGEX).unwrap();
            static ref KERNEL_AMD64_FTYPE_RE: Regex = Regex::new(KERNEL_AMD64_FTYPE_REGEX).unwrap();
            static ref KERNEL_ARMHF_FTYPE_RE: Regex = Regex::new(KERNEL_ARMHF_FTYPE_REGEX).unwrap();
            static ref KERNEL_I386_FTYPE_RE: Regex = Regex::new(KERNEL_I386_FTYPE_REGEX).unwrap();
            static ref DTB_FTYPE_RE: Regex = Regex::new(DTB_FTYPE_REGEX).unwrap();
            static ref GZIP_TAR_FTYPE_RE: Regex = Regex::new(GZIP_TAR_FTYPE_REGEX).unwrap();
        }

        debug!(
            "FileInfo::is_type: looking for: {}, found {}",
            ftype.get_descr(),
            cmd_res.stdout
        );
        match ftype {
            FileType::GZipOSImage => Ok(GZIP_OS_IMG_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::OSImage => Ok(OS_IMG_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::InitRD => Ok(INITRD_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::KernelARMHF => Ok(KERNEL_ARMHF_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::KernelAMD64 => Ok(KERNEL_AMD64_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::KernelI386 => Ok(KERNEL_I386_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::Json => Ok(OS_CFG_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::Text => Ok(TEXT_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::DTB => Ok(DTB_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::GZipTar => Ok(GZIP_TAR_FTYPE_RE.is_match(&cmd_res.stdout)),
        }
    }

    #[cfg(target_os = "windows")]
    pub fn expect_type(&self, ftype: &FileType) -> Result<(), MigError> {
        if !self.is_type(ftype)? {
            let message = format!(
                "Could not determine expected file type '{}' for file '{}'",
                ftype.get_descr(),
                self.path.display()
            );
            error!("{}", message);
            Err(MigError::from_remark(MigErrorKind::InvParam, &message))
        } else {
            Ok(())
        }
    }

    #[cfg(target_os = "windows")]
    pub fn is_type(&self, ftype: &FileType) -> Result<bool, MigError> {
        // TODO: think of something for windows
        Ok(true)
    }

    /*
    pub (crate) fn check_digest(&self, digest: &HashInfo) -> Result<bool, MigError> {
        Ok(check_digest(&self.path, digest)?)
    }
    */
}
