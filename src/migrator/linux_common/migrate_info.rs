use crate::common::{
    FileInfo, 
    OSArch, 
    MigError, 
    MigErrorKind, 
    Stage1Info,
    Stage2Info,
    };

use super::{DiskInfo};


const MODULE: &str = "linux::common::sys_info";

pub(crate) struct MigrateInfo {
    pub os_name: Option<String>,
    // os_release: Option<OSRelease>,
    pub os_arch: Option<OSArch>,
    pub efi_boot: Option<bool>,
    pub secure_boot: Option<bool>,
    pub disk_info: Option<DiskInfo>,
    pub image_info: Option<FileInfo>,
    pub kernel_info: Option<FileInfo>,
    pub initrd_info: Option<FileInfo>,
    pub device_slug: Option<String>,
}

impl<'a> MigrateInfo {
    pub(crate) fn default() -> MigrateInfo {
        MigrateInfo {
            os_name: None,
            // os_release: None,
            os_arch: None,
            efi_boot: None,
            secure_boot: None,
            disk_info: None,
            image_info: None,
            kernel_info: None,
            initrd_info: None,
            device_slug: None,
        }        
    }
}

impl<'a> Stage1Info<'a> for MigrateInfo {
    fn get_os_name(&'a self) -> &'a str {
        if let Some(ref os_name) = self.os_name {
            return os_name;
        }
        panic!("{} uninitialized field os_name in MigrateInfo", MODULE);
    } 

    fn get_drive_size(&self) -> u64 {
        if let Some(ref disk_info) = self.disk_info {
            return disk_info.drive_size;
        }
        panic!("{} uninitialized field drive_info in MigrateInfo", MODULE);
    }

    fn get_boot_path(&'a self) -> &'a str {
        if let Some(ref disk_info) = self.disk_info {
            if let Some(ref boot_path) = disk_info.boot_path {
                return &boot_path.path;
            }
        }
        panic!("{} uninitialized field drive_info in MigrateInfo", MODULE);        
    }

    fn get_boot_device(&'a self) -> &'a str {
        if let Some(ref disk_info) = self.disk_info {
            if let Some(ref boot_path) = disk_info.boot_path {
                return &boot_path.device;
            }
        }
        panic!("{} uninitialized field drive_info in MigrateInfo", MODULE);        
    }

    fn get_root_path(&'a self) -> &'a str {
        if let Some(ref disk_info) = self.disk_info {
            if let Some(ref root_path) = disk_info.root_path {
                return &root_path.path;
            }
        }
        panic!("{} uninitialized field drive_info in MigrateInfo", MODULE);        
    }

    fn get_root_device(&'a self) -> &'a str {
        if let Some(ref disk_info) = self.disk_info {
            if let Some(ref root_path) = disk_info.root_path {
                return &root_path.device;
            }
        }
        panic!("{} uninitialized field drive_info in MigrateInfo", MODULE);        
    }

    fn get_efi_device(&'a self) -> Option<&'a str> {
        if let Some(ref disk_info) = self.disk_info {
            if let Some(ref efi_path) = disk_info.efi_path {
                return Some(&efi_path.device);
            } else {
                return None;
            }
        }
        panic!("{} uninitialized field drive_info in MigrateInfo", MODULE);        
    }


    fn get_device_slug(&'a self) -> &'a str {
        if let Some(ref device_slug) = self.device_slug {
            return device_slug;
        }
        panic!("{} uninitialized field device_slug in MigrateInfo", MODULE);        
    }

}

impl<'a> Stage2Info<'a> for MigrateInfo {    

    fn is_efi_boot(&self) -> bool {
        if let Some(efi_boot) = self.efi_boot {
            efi_boot
        } else {
            false
        }
    }

    fn get_os_arch(&'a self) -> &'a OSArch {
        if let Some(ref os_arch) = self.os_arch {
            return os_arch;
        }
        panic!("{} uninitialized field os_arch in MigrateInfo", MODULE);        
    }

    fn get_work_path(&'a self) -> &'a str {
        if let Some(ref disk_info) = self.disk_info {
            if let Some(ref work_path) = disk_info.work_path {
                return &work_path.path;
            }
        }
        panic!("{} uninitialized field drive_info in MigrateInfo", MODULE);        
    }

    fn get_drive_device(&'a self) -> &'a str {
        if let Some(ref disk_info) = self.disk_info {
            return &disk_info.drive_dev;
        }
        panic!("{} uninitialized field drive_info in MigrateInfo", MODULE);
    }
}

/*
    pub(crate) fn get_work_path(&self) -> Result<&'a str, MigError> {
        if let Some(ref disk_info) = self.disk_info {
            if let Some(work_path) = disk_info.work_path {
                return Ok(&work_path.path);
            }
        }
        Err(MigError::from_remark(MigErrorKind::InvState, "missing root device info"))
    }


    pub(crate) fn get_root_device(&self) -> Result<&'a str, MigError> {
        if let Some(ref disk_info) = self.disk_info {
            if let Some(root_path) = disk_info.root_path {
                return Ok(&root_path.device);
            }
        }
        Err(MigError::from_remark(MigErrorKind::InvState, "missing root device info"))
    }

    pub(crate) fn get_boot_device(&self) -> Result<&'a str, MigError> {
        if let Some(ref disk_info) = self.disk_info {
            if let Some(boot_path) = disk_info.boot_path {
                return Ok(&boot_path.device);
            }
        }
        Err(MigError::from_remark(MigErrorKind::InvState, "missing boot device info"))
    }

    pub(crate) fn get_efi_device(&self) -> Result<&'a str, MigError> {
        if let Some(ref disk_info) = self.disk_info {
            if let Some(efi_path) = disk_info.efi_path {
                return Ok(&efi_path.device);
            }
        }
        Err(MigError::from_remark(MigErrorKind::InvState, "missing efi device info"))
    }
    */
