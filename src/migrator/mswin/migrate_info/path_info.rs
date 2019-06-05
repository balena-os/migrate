use std::path::{PathBuf};
use regex::{Regex};
use lazy_static::{lazy_static};
use log::{warn};

use crate::{
    defs::{FileSystem},
    common::{ MigError, MigErrorKind},
    mswin::{
        wmi_utils::{LogicalDrive, Partition, PhysicalDrive, Volume, physical_drive::DriveType},
    },
};
//use log::{debug, error, info, trace};

// \\?\Volume{e907ceea-7513-4f34-a1d1-fee089d1dd4b}\
const PARTUUID_RE: &str = r#"\\\\\?\\Volume\{([0-9,a-f]{8}-[0-9,a-f]{4}-[0-9,a-f]{4}-[0-9,a-f]{4}-[0-9,a-f]{12})\}\\"#;

const DRIVE_SUFFIX: &[char] = &['a', 'b' , 'c', 'd', 'e'];

#[derive(Debug, Clone)]
pub(crate) struct PathInfo {
    linux_drive: PathBuf,
    part_uuid: Option<String>,
    part_size: u64,
    fs_size: u64,
    fs_free: u64,
    fs_compressed: bool,
    file_system: FileSystem,


    /*
    volume: Volume,
    partition: Partition,
    drive: PhysicalDrive,
    mount: LogicalDrive,
    */
}

impl PathInfo {
    pub fn new(
        volume: &Volume,
        drive: &PhysicalDrive,
        partition: &Partition,
        mount: &LogicalDrive,
    ) -> Result<PathInfo, MigError> {

        lazy_static!{
            static ref PARTUUID_REGEX: Regex = Regex::new(PARTUUID_RE).unwrap();
        }

        let part_uuid = if let Some(captures) = PARTUUID_REGEX.captures(volume.get_device_id()) {
                Some(String::from(captures.get(1).unwrap().as_str()))
            } else {
                warn!("No Part UUID extracted for volume '{}'", volume.get_device_id());
                None
            };

        // TODO: is this likely to work with anything other than the first drive
        // TODO: propper implementation of linux device names 
        let linux_drive =
            match drive.get_drive_type() {
                DriveType::Scsi => {
                    PathBuf::from(format!("/dev/sd{}", DRIVE_SUFFIX[drive.get_index()]))
                },
                DriveType::Ide => {
                    PathBuf::from(format!("/dev/hd{}", DRIVE_SUFFIX[drive.get_index()]))
                },
                DriveType::Other => {
                    return Err(MigError::from_remark(MigErrorKind::NotImpl, &format!("Cannot derive linux drive name from drive type {:?}", drive.get_drive_type())));
                },
        };


        // TODO: extract information rather than copy
        Ok(PathInfo {
            linux_drive,
            part_uuid,
            part_size: partition.get_size(),
            fs_size: mount.get_size(),
            fs_free: mount.get_free_space(),
            fs_compressed: mount.is_compressed(),
            file_system: mount.get_file_system().clone(),

            /*
            volume: volume.clone(),
            partition: partition.clone(),
            drive: drive.clone(),
            mount: mount.clone(),
            */
        })
    }
}
