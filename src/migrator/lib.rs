use log::error;
use nix::unistd::sync;
use std::panic;

use mod_logger::{Level, Logger};

pub mod common;

#[cfg(target_os = "windows")]
mod mswin;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
use linux::stage2::Stage2;

pub(crate) mod defs;

use common::mig_error::MigError;

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
    let res = panic::catch_unwind(|| -> Result<(), MigError> {
        let mut stage2 = match Stage2::try_init() {
            Ok(res) => res,
            Err(why) => {
                error!("Failed to initialize stage2: Error: {}", why);
                Logger::flush();
                sync();
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

        Logger::flush();
        sync();
        stage2.error_exit()?;
        // should not be getting here
        Ok(())
    });

    if let Err(_) = res {
        error!("A panic occurred in stage2");
        Logger::flush();
        sync();
        let _res = Stage2::default_exit();
        ()
    } else {

    }

    Ok(())
}

pub fn test() -> Result<(), MigError> {
    Logger::set_default_level(&Level::Trace);
    /*    ensure_cmds(&[LSBLK_CMD, FDISK_CMD], &[])?;
        linux_common::migrate_info::DiskInfo::new(BootType::GRUB, &Path::new("."), None)?;
    */
    Ok(())
}
