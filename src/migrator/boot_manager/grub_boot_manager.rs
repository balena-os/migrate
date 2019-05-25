use std::path::{Path};
use log::{info, trace, debug, error};

use crate::{
    defs::{BOOT_PATH, ROOT_PATH, NIX_NONE, MIG_KERNEL_NAME, MIG_INITRD_NAME, BALENA_FILE_TAG, GRUB_MIN_VERSION},
    common::{file_exists, is_balena_file, path_append, Config, MigError, MigErrCtx, MigErrorKind,},
    linux_common::{call_cmd, restore_backups, get_grub_version, device_info::{DeviceInfo, path_info::PathInfo}, MKTEMP_CMD, CHMOD_CMD},
    stage2::stage2_config::{Stage2ConfigBuilder, Stage2Config},
    boot_manager::{BootManager, BootType},
};

pub(crate) struct GrubBootManager;

impl GrubBootManager {
    pub fn new() -> GrubBootManager { GrubBootManager{} }
}

impl BootManager for GrubBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::Grub
    }

    fn can_migrate(&self,dev_info: &DeviceInfo, config: &Config, s2_cfg: &mut Stage2ConfigBuilder) -> Result<bool, MigError> {
        trace!("can_migrate: entered");
        let grub_version = get_grub_version()?;
        info!(
            "grub-install version is {}.{}",
            grub_version.0, grub_version.1
        );

        if grub_version.0 < String::from(GRUB_MIN_VERSION) {
            error!("Your version of grub-install ({}.{}) is not supported. balena-migrate requires grub version 2 or higher.", grub_version.0, grub_version.1);
            return Ok(false);
        }

        Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn setup(&self, dev_info: &DeviceInfo, config: &Config, s2_cfg: &mut Stage2ConfigBuilder) -> Result<(), MigError> {
        trace!("setup: entered");
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn restore(&self, slug: &str, root_path: &Path,  config: &Stage2Config) -> Result<(), MigError> {
        trace!("restore: entered");
        Err(MigError::from(MigErrorKind::NotImpl))
    }
/*
    fn set_bootmgr_path(&self,dev_info:& DeviceInfo, config: &Config, s2_cfg: &mut Stage2ConfigBuilder) -> Result<bool, MigError> {
        trace!("set_bootmgr_path: entered");
*/
        /*

match boot_type {
        BootType::EFI => {
            // TODO: this is EFI specific stuff in a non EFI specific place - try to concentrate uboot / EFI stuff in dedicated module
            if let Some(path_info) = PathInfo::new(EFI_PATH, &lsblk_info)? {
                Some(path_info)
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "the device for path '{}' could not be established",
                        EFI_PATH
                    ),
                ));
            }
        }
        BootType::UBoot => DiskInfo::get_uboot_mgr_path(&work_path, &lsblk_info)?,
        _ => None,
    },
*/
/*

        Err(MigError::from(MigErrorKind::NotImpl))
    }
    */
}
