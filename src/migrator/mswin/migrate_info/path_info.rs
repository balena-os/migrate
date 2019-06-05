use std::path::{PathBuf};
use crate::{
    common::{ MigError, MigErrorKind},
    mswin::{
        wmi_utils::{LogicalDrive, Partition, PhysicalDrive, Volume, physical_drive::DriveType},
    },
};
//use log::{debug, error, info, trace};


const DRIVE_SUFFIX: &[char] = &['a', 'b' , 'c', 'd', 'e']; 

#[derive(Debug, Clone)]
pub(crate) struct PathInfo {
    linux_drive: PathBuf,
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
    ) -> Result<PathInfo, MigError> {

        // TODO: is this likely to work with anything other than the first drive
        // TODO: propper implementation of linux device names 
        let linux_drive = PathBuf::from(
            match drive.get_drive_type() {
                DriveType::Scsi => {
                    &format!("/dev/sd{}", DRIVE_SUFFIX[drive.get_index()])
                },
                DriveType::Ide => {
                    &format!("/dev/hd{}", DRIVE_SUFFIX[drive.get_index()])
                },
                DriveType::Other => {
                    return Err(MigError::from_remark(MigErrorKind::NotImpl, &format!("Cannot derive linux drive name from drive type {:?}", drive.get_drive_type())));
                },
        });

        // TODO: extract information rather than copy
        Ok(PathInfo {
            linux_drive,
            volume: volume.clone(),
            partition: partition.clone(),
            drive: drive.clone(),
            mount: mount.clone(),
        })
    }
}
