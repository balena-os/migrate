use std::path::Path;

use crate::{
    common::{BootType, FileInfo, OSArch},
    linux_common::{
        disk_info::{path_info::PathInfo, DiskInfo},
        WifiConfig,
    },
};

const MODULE: &str = "linux_common::migrate_info";

pub(crate) struct MigrateInfo {
    pub os_name: Option<String>,
    // os_release: Option<OSRelease>,
    // pub fail_mode: Option<FailMode>,
    pub os_arch: Option<OSArch>,
    pub boot_type: Option<BootType>,
    pub secure_boot: Option<bool>,
    pub disk_info: Option<DiskInfo>,
    pub install_path: Option<PathInfo>,
    pub os_image_info: Option<FileInfo>,
    pub has_backup: bool,
    pub nwmgr_files: Vec<FileInfo>,
    pub os_config_info: Option<FileInfo>,
    pub kernel_info: Option<FileInfo>,
    pub initrd_info: Option<FileInfo>,
    pub dtb_info: Option<FileInfo>,
    pub device_slug: Option<String>,
    pub boot_cfg_bckup: Vec<(String, String)>,
    pub wifis: Vec<WifiConfig>,
}

impl<'a> MigrateInfo {
    pub(crate) fn default() -> MigrateInfo {
        MigrateInfo {
            os_name: None,
            os_arch: None,
            boot_type: None,
            secure_boot: None,
            disk_info: None,
            install_path: None,
            os_image_info: None,
            has_backup: false,
            os_config_info: None,
            nwmgr_files: Vec::new(),
            kernel_info: None,
            initrd_info: None,
            dtb_info: None,
            device_slug: None,
            boot_cfg_bckup: Vec::new(),
            wifis: Vec::new(),
        }
    }

    pub fn get_install_path(&'a self) -> &'a PathInfo {
        if let Some(ref val) = self.install_path {
            return val;
        }
        panic!("{} uninitialized field install_path in MigrateInfo", MODULE);
    }

    pub(crate) fn get_os_name(&'a self) -> &'a str {
        if let Some(ref os_name) = self.os_name {
            return os_name;
        }
        panic!("{} uninitialized field os_name in MigrateInfo", MODULE);
    }

    pub(crate) fn get_os_arch(&'a self) -> &'a OSArch {
        if let Some(ref os_arch) = self.os_arch {
            return os_arch;
        }
        panic!("{} uninitialized field os_arch in MigrateInfo", MODULE);
    }

    pub fn get_initrd_path(&'a self) -> &'a Path {
        if let Some(ref initrd_info) = self.initrd_info {
            &initrd_info.path
        } else {
            panic!("initrd path is not initialized");
        }
    }

    pub fn get_dtb_path(&'a self) -> Option<&'a Path> {
        if let Some(ref dtb_info) = self.dtb_info {
            Some(&dtb_info.path)
        } else {
            None
        }
    }

    pub fn get_kernel_path(&'a self) -> &'a Path {
        if let Some(ref kernel_info) = self.kernel_info {
            &kernel_info.path
        } else {
            panic!("kernel path is not initialized");
        }
    }

    pub(crate) fn is_efi_boot(&self) -> bool {
        if let Some(ref boot_type) = self.boot_type {
            if let BootType::EFI = boot_type {
                true
            } else {
                false
            }
        } else {
            panic!("{} uninitialized boot_type in MigrateInfo", MODULE);
        }
    }

    pub(crate) fn get_efi_path(&'a self) -> Option<&'a PathInfo> {
        if let Some(ref disk_info) = self.disk_info {
            if let Some(ref efi_path) = disk_info.efi_path {
                return Some(efi_path);
            }
        }
        return None;
    }

    pub(crate) fn get_work_path(&'a self) -> &'a Path {
        if let Some(ref disk_info) = self.disk_info {
            return disk_info.work_path.path.as_path();
        }
        panic!("{} uninitialized field drive_info in MigrateInfo", MODULE);
    }

    pub(crate) fn get_balena_image(&'a self) -> &'a Path {
        if let Some(ref image_info) = self.os_image_info {
            return image_info.path.as_path();
        }
        panic!("{} uninitialized field balena image in MigrateInfo", MODULE);
    }

    pub(crate) fn get_balena_config(&'a self) -> &'a Path {
        if let Some(ref config_info) = self.os_config_info {
            return config_info.path.as_path();
        }
        panic!(
            "{} uninitialized field balena config info in MigrateInfo",
            MODULE
        );
    }

    pub(crate) fn get_device_slug(&'a self) -> &'a str {
        if let Some(ref device_slug) = self.device_slug {
            return device_slug;
        }
        panic!("{} uninitialized field device_slug in MigrateInfo", MODULE);
    }

    pub(crate) fn get_boot_path(&'a self) -> &'a PathInfo {
        if let Some(ref disk_info) = self.disk_info {
            &disk_info.boot_path
        } else {
            panic!("{} uninitialized field drive_info in MigrateInfo", MODULE);
        }
    }

    pub(crate) fn get_root_path(&'a self) -> &'a PathInfo {
        if let Some(ref disk_info) = self.disk_info {
            &disk_info.root_path
        } else {
            panic!("{} uninitialized field drive_info in MigrateInfo", MODULE);
        }
    }

    pub(crate) fn get_disk_info(&'a self) -> &'a DiskInfo {
        if let Some(ref disk_info) = self.disk_info {
            disk_info
        } else {
            panic!("{} uninitialized field drive_info in MigrateInfo", MODULE);
        }
    }
}
