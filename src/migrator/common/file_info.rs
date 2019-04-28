// expecting work_path to be absolute
use failure::ResultExt;
use lazy_static::lazy_static;
use log::{error, trace};
use regex::Regex;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};

const OS_IMG_FTYPE_REGEX: &str = r#"^DOS/MBR boot sector.*\(gzip compressed data.*\)$"#;
const INITRD_FTYPE_REGEX: &str = r#"^ASCII cpio archive.*\(gzip compressed data.*\)$"#;
const OS_CFG_FTYPE_REGEX: &str = r#"^ASCII text.*$"#;
const KERNEL_AMD64_FTYPE_REGEX: &str = r#"^Linux kernel x86 boot executable bzImage.*$"#;
const KERNEL_ARMHF_FTYPE_REGEX: &str = r#"^Linux kernel ARMHF boot executable bzImage.*$"#;
const KERNEL_I386_FTYPE_REGEX: &str = r#"^Linux kernel i386 boot executable bzImage.*$"#;

#[cfg(target_os = "linux")]
use crate::migrator::linux::util::{call_cmd, FILE_CMD};

use crate::migrator::common::{MigErrCtx, MigError, MigErrorKind};

const MODULE: &str = "migrator::common::file_info";

#[derive(Debug)]
pub enum FileType {
    OSImage,
    KernelAMD64,
    KernelARMHF,
    KernelI386,
    InitRD,
    Json,
}

#[derive(Debug)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
    pub in_work_dir: bool,
}

// TODO: make this detect file formats used by migrate, eg: kernel, initramfs, json file, disk image

impl FileInfo {
    pub fn new(file: &str, work_dir: &str) -> Result<Option<FileInfo>, MigError> {
        trace!(
            "FileInfo::new: entered with file: '{}', work_dir: '{}'",
            file,
            work_dir
        );

        let file_path = PathBuf::from(file);
        let work_path = Path::new(work_dir);

        // figure out if this a path relative to work_dir rather than absolute or relative to current dir

        let checked_path = if file_path.is_absolute()
            || file_path.starts_with("./")
            || file_path.starts_with("../")
            || ((MAIN_SEPARATOR != '/')
                && (file_path.starts_with(&format!(".{}", MAIN_SEPARATOR))
                    || file_path.starts_with(&format!("..{}", MAIN_SEPARATOR))))
        {
            file_path
        } else {
            work_path.join(file_path)
        };

        if checked_path.exists() {
            let abs_path = checked_path.canonicalize().unwrap();
            let metadata = abs_path.metadata().context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("failed to retrieve metadata for path {:?}", abs_path),
            ))?;
            let path = String::from(abs_path.to_str().unwrap());
            Ok(Some(FileInfo {
                path: path,
                size: metadata.len(),
                in_work_dir: false,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn expect_type(&self, ftype: &FileType) -> Result<(), MigError> {
        if !self.is_type(ftype)? {
            let message = format!(
                "Could not determine expected file type {:?} for file '{}'",
                ftype, &self.path
            );
            error!("{}", message);
            Err(MigError::from_remark(MigErrorKind::InvParam, &message))
        } else {
            Ok(())
        }
    }

    #[cfg(target_os = "linux")]
    pub fn is_type(&self, ftype: &FileType) -> Result<bool, MigError> {
        let args: Vec<&str> = vec!["-bz", &self.path];
        let cmd_res = call_cmd(FILE_CMD, &args, true)?;
        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::new: failed determine type for file {}",
                    MODULE, &self.path
                ),
            ));
        }

        lazy_static! {
            static ref OS_IMG_FTYPE_RE: Regex = Regex::new(OS_IMG_FTYPE_REGEX).unwrap();
            static ref INITRD_FTYPE_RE: Regex = Regex::new(INITRD_FTYPE_REGEX).unwrap();
            static ref OS_CFG_FTYPE_RE: Regex = Regex::new(OS_CFG_FTYPE_REGEX).unwrap();
            static ref KERNEL_AMD64_FTYPE_RE: Regex = Regex::new(KERNEL_AMD64_FTYPE_REGEX).unwrap();
            static ref KERNEL_ARMHF_FTYPE_RE: Regex = Regex::new(KERNEL_ARMHF_FTYPE_REGEX).unwrap();
            static ref KERNEL_I386_FTYPE_RE: Regex = Regex::new(KERNEL_I386_FTYPE_REGEX).unwrap();
        }

        match ftype {
            FileType::OSImage => Ok(OS_IMG_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::InitRD => Ok(INITRD_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::KernelARMHF => Ok(KERNEL_ARMHF_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::KernelAMD64 => Ok(KERNEL_AMD64_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::KernelI386 => Ok(KERNEL_I386_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::Json => Ok(OS_CFG_FTYPE_RE.is_match(&cmd_res.stdout)),
        }
    }

    #[cfg(target_os = "windows")]
    pub fn is_type(ftype: FileType) -> Result<bool, MigError> {
        // think of something for windows
        Err(MigError::from(MigErrorKind::NotImpl))
    }
}
