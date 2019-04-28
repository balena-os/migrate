// expecting work_path to be absolute
use failure::ResultExt;
use log::{debug, trace};
use std::path::{Path, PathBuf, MAIN_SEPARATOR};

#[cfg(target_os = "linux")]
use crate::migrator::linux::util::{call_cmd, FILE_CMD};

use crate::migrator::common::{MigErrCtx, MigError, MigErrorKind};

const MODULE: &str = "migrator::common::file_info";

#[derive(Debug)]
pub struct FileInfo {
    pub path: String,
    pub ftype: Option<String>,
    pub size: u64,
    pub in_work_dir: bool,
}

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
                ftype: FileInfo::get_file_type(&path)?,
                path: path,
                size: metadata.len(),
                in_work_dir: false,
            }))
        } else {
            Ok(None)
        }
    }

    #[cfg(target_os = "windows")]
    fn get_file_type(_path: &str) -> Result<Option<String>, MigError> {
        // think of something for windows
        Ok(None)
    }

    #[cfg(target_os = "linux")]
    fn get_file_type(path: &str) -> Result<Option<String>, MigError> {
        let args: Vec<&str> = vec!["-bz", path];
        let cmd_res = call_cmd(FILE_CMD, &args, true)?;
        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("{}::new: failed determine type for file {}", MODULE, path),
            ));
        }
        Ok(Some(String::from(cmd_res.stdout)))
    }
}
