use crate::common::{MigError};
use super::MigrateInfo;

pub(crate) trait DeviceStage1 {
    fn get_device_slug(&self) -> &'static str;
    fn setup(&self, mig_info: &mut MigrateInfo) -> Result<(),MigError>;
}
