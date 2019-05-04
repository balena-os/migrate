use std::path::PathBuf;

use super::PathInfo;

pub(crate) struct DiskInfo {
    pub drive_dev: PathBuf,
    pub drive_size: u64,
    pub drive_uuid: String,
    pub root_path: Option<PathInfo>,
    pub boot_path: Option<PathInfo>,
    pub efi_path: Option<PathInfo>,
    pub work_path: Option<PathInfo>,
}

impl DiskInfo {
    pub(crate) fn default() -> DiskInfo {
        DiskInfo {
            drive_dev: PathBuf::from(""),
            drive_uuid: String::from(""),
            drive_size: 0,
            root_path: None,
            boot_path: None,
            efi_path: None,
            work_path: None,
        }
    }
}
