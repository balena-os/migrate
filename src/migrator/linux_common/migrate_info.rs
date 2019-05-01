use std::fs::File;
use std::io::Write;
use failure::{ResultExt};

use crate::common::{
    FileInfo, 
    OSArch, 
    STAGE2_CFG_FILE,
    MigError, 
    MigErrCtx,
    MigErrorKind, 
    Stage1Info,
    Stage2Info,
    stage_info::{
        EFI_BOOT_KEY,
        ROOT_DEVICE_KEY,        
        BOOT_DEVICE_KEY,        
        DEVICE_SLUG_KEY,
        BALENA_IMAGE_KEY,        
        BALENA_CONFIG_KEY,
        BACKUP_CONFIG_KEY,
        BACKUP_ORIG_KEY,
        BACKUP_BCKUP_KEY,
    },
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
    pub os_image_info: Option<FileInfo>,
    pub os_config_info: Option<FileInfo>,
    pub kernel_info: Option<FileInfo>,
    pub initrd_info: Option<FileInfo>,
    pub device_slug: Option<String>,
    pub boot_cfg_bckup: Vec<(String,String)>,
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
            os_image_info: None,
            os_config_info: None,
            kernel_info: None,
            initrd_info: None,
            device_slug: None,
            boot_cfg_bckup: Vec::new(),
        }        
    }

    pub fn write_stage2_cfg(&self) -> Result<(),MigError> {        
        let mut cfg_str = String::from( "# Balena Migrate Stage2 Config\n");
        cfg_str.push_str(&format!(      "{}: {}\n", EFI_BOOT_KEY, self.is_efi_boot()));
        cfg_str.push_str(&format!(      "{}: '{}'\n", DEVICE_SLUG_KEY, self.get_device_slug()));        
        //cfg_str.push_str(&format!(      "{}: '{}'\n", DRIVE_DEVICE_KEY, self.get_drive_device()));        
        cfg_str.push_str(&format!(      "{}: '{}'\n", BALENA_IMAGE_KEY ,self.get_balena_image()));        
        cfg_str.push_str(&format!(      "{}: '{}'\n", BALENA_CONFIG_KEY, self.get_balena_config()));        
        cfg_str.push_str(&format!(      "{}: '{}'\n", ROOT_DEVICE_KEY, self.get_root_device()));        
        cfg_str.push_str(&format!(      "{}: '{}'\n", BOOT_DEVICE_KEY, self.get_boot_device()));        
        cfg_str.push_str(               "# backed up files in boot config\n");
        cfg_str.push_str(&format!(      "{}:\n", BACKUP_CONFIG_KEY));        
        for bckup in &self.boot_cfg_bckup {            
            cfg_str.push_str(&format!(  "  - {}:      '{}'\n", BACKUP_ORIG_KEY, &bckup.0 ));        
            cfg_str.push_str(&format!(  "    {}:     '{}'\n",BACKUP_BCKUP_KEY, &bckup.1 ));        
        }
        let mut cfg_file = File::create(STAGE2_CFG_FILE).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("failed to create new stage 2 config file '{}'", STAGE2_CFG_FILE)))?;
        cfg_file.write_all(cfg_str.as_bytes()).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("failed to write new  stage 2 config file '{}'", STAGE2_CFG_FILE)))?;
        
        Ok(())
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

    fn get_work_path(&'a self) -> &'a str {
        if let Some(ref disk_info) = self.disk_info {
            if let Some(ref work_path) = disk_info.work_path {
                return &work_path.path;
            }
        }
        panic!("{} uninitialized field drive_info in MigrateInfo", MODULE);        
    }
    
    fn get_os_arch(&'a self) -> &'a OSArch {
        if let Some(ref os_arch) = self.os_arch {
            return os_arch;
        }
        panic!("{} uninitialized field os_arch in MigrateInfo", MODULE);        
    }

    fn get_drive_device(&'a self) -> &'a str {
        if let Some(ref disk_info) = self.disk_info {
            return &disk_info.drive_dev;
        }
        panic!("{} uninitialized field drive_info in MigrateInfo", MODULE);
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


    fn get_balena_image(&'a self) -> &'a str {
        if let Some(ref image_info) = self.os_image_info {
            return &image_info.path;
        }
        panic!("{} uninitialized field balena image in MigrateInfo", MODULE);        
    }

    fn get_balena_config(&'a self) -> &'a str {
        if let Some(ref config_info) = self. os_config_info {
            return &config_info.path;
        }
        panic!("{} uninitialized field balena config info in MigrateInfo", MODULE);        
    }


    fn get_device_slug(&'a self) -> &'a str {
        if let Some(ref device_slug) = self.device_slug {
            return device_slug;
        }
        panic!("{} uninitialized field device_slug in MigrateInfo", MODULE);        
    }

    fn get_backups(&'a self) -> &'a Vec<(String,String)> {
        &self.boot_cfg_bckup
    }

    fn get_boot_device(&'a self) -> &'a str {
        if let Some(ref disk_info) = self.disk_info {
            if let Some(ref boot_path) = disk_info.boot_path {
                return &boot_path.device;
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
