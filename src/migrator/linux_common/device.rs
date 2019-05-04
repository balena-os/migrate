use super::MigrateInfo;
use crate::common::{Config, MigError}; // , Stage2Info};
use crate::stage2::Stage2Config;
use std::path::Path;

pub(crate) trait Device {
    fn get_device_slug(&self) -> &'static str;
    fn setup(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError>;
    fn restore_boot(&self, root_path: &Path, config: &Stage2Config) -> Result<(), MigError>;
}
