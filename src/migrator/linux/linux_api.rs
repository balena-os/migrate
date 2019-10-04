use failure::ResultExt;
use std::path::{Path, PathBuf};

use crate::{
    common::{os_api::OSApiImpl, path_info::PathInfo, MigErrCtx, MigError, MigErrorKind},
    defs::{FileType, OSArch},
    linux::{
        linux_common::{expect_type, get_os_arch, get_os_name},
        lsblk_info::LsblkInfo,
    },
};

pub(crate) struct LinuxAPI {
    lsblk_info: LsblkInfo,
}

impl LinuxAPI {
    pub fn new() -> Result<LinuxAPI, MigError> {
        Ok(LinuxAPI {
            lsblk_info: LsblkInfo::all()?,
        })
    }

    pub fn get_lsblk_info(&self) -> LsblkInfo {
        self.lsblk_info.clone()
    }
}

impl OSApiImpl for LinuxAPI {
    fn cannonicalize<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, MigError> {
        Ok(path
            .as_ref()
            .canonicalize()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Unable to canonicalize path: '{}'", path.as_ref().display()),
            ))?)
    }

    fn get_os_arch(&self) -> Result<OSArch, MigError> {
        get_os_arch()
    }

    fn get_os_name(&self) -> Result<String, MigError> {
        get_os_name()
    }

    fn path_info_from_path<P: AsRef<Path>>(&self, path: P) -> Result<PathInfo, MigError> {
        PathInfo::from_lsblk_info(path, &self.lsblk_info)
    }

    fn expect_type<P: AsRef<Path>>(&self, file: P, ftype: &FileType) -> Result<(), MigError> {
        expect_type(file.as_ref(), ftype)
    }
}
