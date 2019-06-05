use crate::{
    common::{dir_exists, os_release::OSRelease, path_append, Config, MigError, MigErrorKind},
    defs::OSArch,
    mswin::{
        powershell::PSInfo,
        util::mount_efi,
        win_api::is_efi_boot,
        wmi_utils::{LogicalDrive, Partition, PhysicalDrive, Volume, WmiUtils},
    },
};
use log::{debug, error, info, trace};

#[derive(Debug, Clone)]
pub(crate) struct PathInfo {
    volume: Volume,
    partition: Partition,
    drive: PhysicalDrive,
    mount: LogicalDrive,
}

impl PathInfo {
    pub fn new(
        volume: &Volume,
        drive: &PhysicalDrive,
        partition: &Partition,
        mount: &LogicalDrive,
    ) -> Resul<PathInfo, MigError> {
        // TODO: extract information rather than copy
        Ok(PathInfo {
            volume: volume.clone(),
            partition: partition.clone(),
            drive: drive.clone(),
            mount: moun.clone(),
        })
    }
}
