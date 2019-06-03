use log::error;

use mod_logger::{Logger, Level};
// use std::path::Path;

pub mod common;

#[cfg(target_os = "windows")]
mod mswin;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
use linux::stage2::Stage2;

pub(crate) mod defs;

//pub(crate) use common::config::{Config, YamlConfig};
// use crate::linux_common::{ensure_cmds, FDISK_CMD, LSBLK_CMD};
use common::mig_error::MigError;
//pub(crate) use common::os_release::OSRelease;
//pub(crate) use common::OSArch;

//pub(crate) const MODULE: &str = "balena_migrate";

#[cfg(target_os = "windows")]
pub fn migrate() -> Result<(), MigError> {
    Ok(mswin::MSWMigrator::migrate()?)
}

#[cfg(target_os = "linux")]
pub fn migrate() -> Result<(), MigError> {
    Ok(linux::LinuxMigrator::migrate()?)
}

#[cfg(target_os = "linux")]
pub fn stage2() -> Result<(), MigError> {
    let mut stage2 = match Stage2::try_init() {
        Ok(res) => res,
        Err(why) => {
            error!("Failed to initialize stage2: Error: {}", why);
            Stage2::default_exit()?;
            // should not be getting here
            return Ok(());
        }
    };

    match stage2.migrate() {
        Ok(_res) => {
            error!("stage2::migrate() is not expected to return on success");
        }
        Err(why) => {
            error!("Failed to complete stage2::migrate Error: {}", why);
        }
    }

    stage2.error_exit()?;
    // should not be getting here
    Ok(())
}

pub fn test() -> Result<(), MigError> {
    Logger::set_default_level(&Level::Trace);
    /*    ensure_cmds(&[LSBLK_CMD, FDISK_CMD], &[])?;
        linux_common::migrate_info::DiskInfo::new(BootType::GRUB, &Path::new("."), None)?;
    */
    Ok(())
}
