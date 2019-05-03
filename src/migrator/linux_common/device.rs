use std::path::{Path};
use crate::common::{MigError, Config}; // , Stage2Info};
use crate::stage2::{Stage2Config};
use super::MigrateInfo;

pub(crate) trait Device {
    fn get_device_slug(&self) -> &'static str;
    fn setup(&self, config: &Config, mig_info: &mut MigrateInfo) -> Result<(),MigError>;
    fn restore_boot(&self, root_path: &Path, config: &Stage2Config) -> Result<(),MigError>;
}
