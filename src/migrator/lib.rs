#[cfg(target_os = "windows")]
mod mswin;

#[cfg(target_os = "linux")]
mod linux;

mod common;

pub use common::config::{Config, YamlConfig};
pub use common::mig_error::{MigErrCtx, MigError, MigErrorKind};
pub use common::os_release::OSRelease;
pub use common::OSArch;

pub(crate) const MODULE: &str = "balena_migrate";

#[cfg(target_os = "windows")]
pub fn migrate() -> Result<(), MigError> {
    Ok(mswin::MSWMigrator::migrate()?)
}

#[cfg(target_os = "linux")]
pub fn migrate() -> Result<(), MigError> {
    Ok(linux::LinuxMigrator::migrate()?)
}
