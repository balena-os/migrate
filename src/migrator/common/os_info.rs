use std::path::{Path};

use crate::{
    common::{MigError, file_info::FileType},
    defs::{OSArch},
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathInfo {
   // TODO: drive, partition, mountpoint, drive & partition sizes, fs type, fs size, fs free, part uuids, labels
}

pub trait OSInfo {

    fn is_admin(&self) -> Result<bool,MigError>;
    fn get_os_arch(&self) -> Result<OSArch, MigError>;
    fn get_os_name(&self) -> Result<String, MigError>;

    // TODO: call command interface incl ensured commands

    // Disk specific calls
    fn get_path_info(&self, path: &Path ) -> Result<PathInfo, MigError>;
    fn get_boot_info(&self) -> Result<PathInfo, MigError>;
    fn get_install_drive_info(&self) -> Result<PathInfo, MigError>;

    // file types
    fn is_file_type(&self, ftype: FileType) -> Result<bool, MigError>;

}