use std::path::{Path, PathBuf};

use crate::{
    common::{file_info::FileType, MigError},
    defs::OSArch,
};

pub trait PathInfo {
    fn get_path(&self) -> &Path;
    fn get_drive(&self) -> &Path;
    fn get_partition(&self) -> &Path;
    fn get_mountpoint(&self) -> &Path;
    fn get_drive_size(&self) -> u64;
    fn get_fs_type(&self) -> &str;
    fn get_fs_size(&self) -> u64;
    fn get_fs_free(&self) -> u64;
    fn get_uuid(&self) -> Option<&str>;
    fn get_part_uuid(&self) -> Option<&str>;
    fn get_label(&self) -> Option<&str>;
}

pub trait OSInfo {
    fn is_admin(&self) -> Result<bool, MigError>;
    fn get_os_arch(&self) -> Result<OSArch, MigError>;
    fn get_os_name(&self) -> Result<String, MigError>;

    // TODO: call command interface incl ensured commands

    //fn get_mem_info(&self) -> Result<(u64, u64), MigError>;

    // Disk specific calls
    fn path_info_from_path<P: AsRef<Path>>(&self, path: P) -> Result<dyn PathInfo, MigError>;
    fn path_info_from_partition<P: AsRef<Path>>(&self, partition: P) -> Result<dyn PathInfo, MigError>;
    fn get_boot_info(&self) -> Result<dyn PathInfo, MigError>;
    fn get_install_drive_info(&self) -> Result<dyn PathInfo, MigError>;

    // file types
    fn is_file_type(&self, ftype: FileType) -> Result<bool, MigError>;
}
