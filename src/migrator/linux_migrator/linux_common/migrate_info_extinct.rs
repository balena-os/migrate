use std::path::Path;

use crate::{
    common::{FileInfo, OSArch},
    linux_common::{
        device_info::{path_info::PathInfo, DeviceInfo},
        WifiConfig,
    },
    boot_manager::{BootManager, BootType},
};
use crate::linux_common::device_info::lsblk_info::LsblkInfo;

// const MODULE: &str = "linux_common::migrate_info";

pub(crate) struct MigrateInfo {
    pub os_name: Option<String>,
    pub os_arch: Option<OSArch>,
    pub secure_boot: Option<bool>,
    pub disk_info: Option<DeviceInfo>,
    pub install_path: Option<PathInfo>,
    pub os_image_info: Option<FileInfo>,
    pub has_backup: bool,
    pub nwmgr_files: Vec<FileInfo>,
    pub os_config_info: Option<FileInfo>,
    pub kernel_info: Option<FileInfo>,
    pub initrd_info: Option<FileInfo>,
    pub dtb_info: Option<FileInfo>,
    pub boot_cfg_bckup: Vec<(String, String)>,
    pub wifis: Vec<WifiConfig>,

}

impl<'a> MigrateInfo {
    pub(crate) fn default() -> MigrateInfo {
        MigrateInfo {
            os_name: None,
            os_arch: None,
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
            boot_cfg_bckup: Vec::new(),
            wifis: Vec::new(),
        }
    }


    // **************************************************
    // getter functoins

    pub(crate) fn get_os_name(&'a self) -> &'a str {
        if let Some(ref os_name) = self.os_name {
            return os_name;
        }
        panic!("Uninitialized field os_name in MigrateInfo");
    }

    pub(crate) fn get_os_arch(&'a self) -> &'a OSArch {
        if let Some(ref os_arch) = self.os_arch {
            return os_arch;
        }
        panic!("Uninitialized field os_arch in MigrateInfo");
    }

    pub fn get_lsblk_info(&'a self) -> &'a LsblkInfo {
        if let Some(ref diskinfo) = self.disk_info {
            return &diskinfo.lsblk_info;
        }
        panic!("Uninitialized field install_path in MigrateInfo");
    }


    // ***************************************************
    // get PathInfos for root, boot, install, bootmgr,

    pub(crate) fn get_disk_info(&'a self) -> &'a DeviceInfo {
        if let Some(ref disk_info) = self.disk_info {
            disk_info
        } else {
            panic!("Uninitialized field drive_info in MigrateInfo");
        }
    }

    pub(crate) fn get_boot_pi(&'a self) -> &'a PathInfo {
        &self.get_disk_info().boot_path
    }

    pub(crate) fn get_root_pi(&'a self) -> &'a PathInfo {
        if let Some(ref disk_info) = self.disk_info {
            &disk_info.root_path
        } else {
            panic!("Uninitialized field drive_info in MigrateInfo");
        }
    }

    pub fn get_install_pi(&'a self) -> &'a PathInfo {
        if let Some(ref diskinfo) = self.disk_info {
            return &diskinfo.inst_path;
        }
        panic!("Uninitialized field install_path in MigrateInfo");
    }

    pub(crate) fn get_bootmgr_pi(&'a self) -> Option<&'a PathInfo> {
        if let Some(ref disk_info) = self.disk_info {
            if let Some(ref bootmgr_path) = disk_info.bootmgr_path {
                return Some(bootmgr_path);
            }
        }
        return None;
    }


    pub fn set_bootmgr_pi(&mut self, bootmgr: Option<PathInfo>) {
        if let Some(ref mut diskinfo) = self.disk_info {
            diskinfo.bootmgr_path = bootmgr;
        }
        panic!("Uninitialized field install_path in MigrateInfo");
    }

    // ***************************************************
    // Get paths to installable items, workdir

    pub(crate) fn get_work_path(&'a self) -> &'a Path {
        if let Some(ref disk_info) = self.disk_info {
            return disk_info.work_path.path.as_path();
        }
        panic!("Uninitialized field drive_info in MigrateInfo");
    }

    pub fn get_initrd_path(&'a self) -> &'a Path {
        if let Some(ref initrd_info) = self.initrd_info {
            &initrd_info.path
        } else {
            panic!("Initrd path is not initialized");
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
            panic!("Kernel path is not initialized");
        }
    }

    pub(crate) fn get_image_path(&'a self) -> &'a Path {
        if let Some(ref image_info) = self.os_image_info {
            return image_info.path.as_path();
        }
        panic!("Uninitialized field balena image in MigrateInfo");
    }

    pub(crate) fn get_config_path(&'a self) -> &'a Path {
        if let Some(ref config_info) = self.os_config_info {
            return config_info.path.as_path();
        }
        panic!(
            "Uninitialized field balena config info in MigrateInfo",
        );
    }


    pub(crate) fn is_efi_boot(&self) -> bool {
        if let Some(ref boot_manager) = self.boot_manager {
            if let BootType::Efi = boot_manager.get_boot_type() {
                true
            } else {
                false
            }
        } else {
            panic!("Uninitialized boot_type in MigrateInfo");
        }
    }

    pub(crate) fn get_boot_manager(&'a self) -> &'a Box<BootManager> {
        if let Some(ref bootmgr) = self.boot_manager {
            return bootmgr;
        } else {
            panic!("uninitialized boot_manager in MigrateInfo");
        }
    }

/*
    pub(crate) fn get_device_slug(&'a self) -> &'a str {
        if let Some(ref device_slug) = self.device_slug {
            return device_slug;
        }
        panic!("Uninitialized field device_slug in MigrateInfo");
    }
*/
}
