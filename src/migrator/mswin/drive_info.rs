use regex::{Captures, Regex};
use std::mem::transmute;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, Once};

use crate::mswin::win_api::is_efi_boot;
use crate::{
    common::{
        call, device_info::DeviceInfo, file_exists, path_append, path_info::PathInfo, MigError,
        MigErrorKind,
    },
    mswin::{
        util::mount_efi,
        win_api::{get_volume_disk_extents, is_efi_boot, is_efi_boot, DiskExtent},
        wmi_utils::{LogicalDrive, Partition, PhysicalDrive, Volume},
    },
};
use nix::NixPath;
use std::path::PathBuf;

// \\?\Volume{345ad334-48a8-11e8-9eaf-806e6f6e6963}\

const VOL_UUID_REGEX: &str =
    r#"^\\\\\?\\Volume\{([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})\}\\$"#;

struct VolumeInfo {
    pub volume: Volume,
    pub logical_drive: LogicalDrive,
    pub physical_drive: PhysicalDrive,
    pub partition: Partition,
}

struct SharedDriveInfo {
    volumes: Option<Vec<VolumeInfo>>,
}

pub(crate) struct DriveInfo {
    inner: Arc<Mutex<SharedDriveInfo>>,
}

impl DriveInfo {
    pub fn new() -> Result<DriveInfo, MigError> {
        static mut DRIVE_INFO: *const DriveInfo = 0 as *const DriveInfo;
        static ONCE: Once = Once::new();

        let drive_info = unsafe {
            ONCE.call_once(|| {
                // Make it
                //dbg!("call_once");
                let singleton = DriveInfo {
                    inner: Arc::new(Mutex::new(None)),
                };

                // Put it in the heap so it can outlive this call
                DRIVE_INFO = transmute(Box::new(singleton));
            });

            (*DRIVE_INFO).clone()
        };

        let _dummy = drive_info.init()?;
        drive_info
    }

    pub fn path_info_from_path<P: AsRef<Path>>(&self, path: P) -> Result<PathInfo, MigError> {
        let drive_info = self.init()?;
        if let Some(found) = drive_info
            .volumes
            .unwrap()
            .iter()
            .find(|di| PathBuf::from(di.logical_drive.get_name()).starts_with(path.as_ref()))
        {
            let dev_info = DeviceInfo {
                // the drive device path
                drive: String::from(found.physical_drive.get_device_id()),
                // the drive size
                drive_size: found.physical_drive.get_size(),
                // the partition device path
                device: String::from(found.volume.get_device_id()),
                // TODO: the partition index - this value is not correct in windows as hidden partotions are not counted
                index: found.partition.get_part_index() as u16,
                // the partition fs type
                fs_type: String::from(found.volume.get_file_system().to_linux_str()),
                // the partition uuid
                uuid: None,
                // the partition partuuid
                part_uuid: if let Some(captures) = Regex::new(VOL_UUID_REGEX)
                    .unwrap()
                    .captures(found.volume.get_device_id())
                {
                    Some(String::from(captures.get(1).unwrap().as_str()))
                } else {
                    None
                },
                // the partition label
                part_label: if let Some(label) = found.volume.get_label() {
                    Some(String::from(label))
                } else {
                    None
                },
                // the partition size
                part_size: found.partition.get_size(),
            };

            Ok(PathInfo {
                // the physical device info
                device_info,
                // the absolute path
                path: path.as_ref().to_path_buf(),
                // the devices mountpoint
                mountpoint: PathBuf::from(found.logical_drive.get_name()),
                // the partition read only flag
                // pub mount_ro: bool,
                // The file system size
                fs_size: if let Some(capacity) = found.volume.get_capacity() {
                    capacity
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!(
                            "No fs capacity found for path '{}'",
                            path.as_ref().display()
                        ),
                    ));
                },
                // the fs free space
                fs_free: if let Some(free_space) = found.volume.get_free_space() {
                    free_space
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!(
                            "No fs free_space found for path '{}'",
                            path.as_ref().display()
                        ),
                    ));
                },
            })
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "No logical drive found for path '{}'",
                    path.as_ref().display()
                ),
            ))
        }
    }

    fn init(&self) -> Result<MutexGuard<SharedDriveInfo>, MigError> {
        let mut shared_di = self.inner.lock().unwrap();
        if shared_di.volumes.is_some() {
            return Ok(shared_di);
        }

        let efi_drive = if is_efi_boot() {
            Some(mount_efi()?)
        } else {
            None
        };

        let mut vol_infos: Vec<VolumeInfo> = Vec::new();

        let volumes = Volume::query_all()?;
        // Detect  EFI, boot drive
        for volume in volumes {
            // get DiskPartition for volume
            let disk_extents = get_volume_disk_extents(volume.get_device_id())?;

            if disk_extents.len() == 1 {
                let disk_extent = &disk_extents[0];
                let physical_drive = PhysicalDrive::by_index(disk_extent.disk_index as usize)?;
                if let Some(partition) = physical_drive
                    .query_partitions()?
                    .iter()
                    .find(|p| p.get_start_offset() == disk_extent.start_offset as u64)
                {
                    // got volume & partition here
                    let logical_drive = if let Some(drive_letter) = volume.get_drive_letter() {
                        Some(LogicalDrive::query_for_name(drive_letter)?)
                    } else {
                        if volume.is_system() {
                            efi_drive
                        } else {
                            None
                        }
                    };

                    if let Some(logical_drive) = logical_drive {
                        // Her we have all components: PhysicalDrive, Volume, Partition, LogicalDrive
                        vol_infos.push(VolumeInfo {
                            volume,
                            physical_drive,
                            logical_drive,
                            partition: partition.clone(),
                        });
                    }
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::NotFound,
                        &format!(
                            "Could not find partition for disk index: {}, start offset: {}",
                            disk_extent.disk_index, disk_extent.start_offset
                        ),
                    ));
                }
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "Encountered invalid number of disk extents ({} != 1) for volume '{}'",
                        disk_extents.len(),
                        volume.get_device_id()
                    ),
                ));
            }
        }

        shared_di.volumes = Some(vol_infos);
        Ok(shared_di)
    }
}
