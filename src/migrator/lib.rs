use log::error;

mod stage2;
use stage2::Stage2;
mod common;

#[cfg(target_os = "windows")]
mod mswin;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod linux_common;
#[cfg(target_os = "linux")]
mod device;


pub(crate) mod defs;

//pub(crate) use common::config::{Config, YamlConfig};
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
