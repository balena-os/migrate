use failure::ResultExt;
use std::fs;
use std::path::Path;

use crate::{
    common::{
        MigError,
        migrate_info::MigrateInfo, path_info::PathInfo, stage2_config::Stage2ConfigBuilder, Config,
    },
    defs::BootType,
};

#[cfg(target_os = "linux")]
use crate::{common::{
    MigErrCtx, MigErrorKind,
    stage2_config::Stage2Config
}, linux::stage2::mounts::Mounts};

pub(crate) trait BootManager {
    fn get_boot_type(&self) -> BootType;
    fn can_migrate(
        &mut self,
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError>;

    fn setup(
        &mut self,
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
        kernel_opts: &str,
    ) -> Result<(), MigError>;

    #[cfg(target_os = "linux")]
    fn restore(&self, mounts: &Mounts, config: &Stage2Config) -> bool;

    // TODO: make return reference
    fn get_bootmgr_path(&self) -> PathInfo;
}

impl dyn BootManager {
    // helper function for implementations

    // get required space for replacing src file with dest file
    #[cfg(target_os = "linux")]
    pub fn get_file_required_space(src: &Path, dest: &Path) -> Result<u64, MigError> {
        if src.exists() {
            let required_space = fs::metadata(src)
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("unable to retrieve size for file '{}'", src.display()),
                ))?
                .len();
            if dest.exists() {
                Ok(std::cmp::max(
                    0,
                    required_space
                        - fs::metadata(dest)
                            .context(MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!("unable to retrieve size for file '{}'", dest.display()),
                            ))?
                            .len(),
                ))
            } else {
                Ok(required_space)
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("Required file '{}' could not be found", src.display()),
            ))
        }
    }
}
