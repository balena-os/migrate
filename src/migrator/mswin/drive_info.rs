use regex::{Captures, Regex};
use std::mem::transmute;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, Once};

use crate::{
    common::{
        call, device_info::DeviceInfo, file_exists, path_append, path_info::PathInfo, MigError,
        MigErrorKind,
    },
    mswin::{
        util::mount_efi,
        win_api::{get_volume_disk_extents, is_efi_boot, DiskExtent},
        wmi_utils::{LogicalDrive, Partition, PhysicalDrive, Volume},
    },
};

// \\?\Volume{345ad334-48a8-11e8-9eaf-806e6f6e6963}\

const VOL_UUID_REGEX: &str =
    r#"^\\\\\?\\Volume\{([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})\}\\$"#;

struct VolumeInfo {
    pub part_uuid: String,
    pub volume: Volume,
    pub logical_drive: LogicalDrive,
    pub physical_drive: PhysicalDrive,
    pub partition: Partition,
}

impl VolumeInfo {
    fn to_device_info(&self) -> DeviceInfo {
        DeviceInfo {
            // the drive device path
            drive: String::from(self.physical_drive.get_device_id()),
            // the devices mountpoint
            mountpoint: PathBuf::from(self.logical_drive.get_name()),
            // the drive size
            drive_size: self.physical_drive.get_size(),
            // the partition device path
            device: String::from(self.volume.get_device_id()),
            // TODO: the partition index - this value is not correct in windows as hidden partotions are not counted
            index: None,
            // the partition fs type
            fs_type: String::from(self.volume.get_file_system().to_linux_str()),
            // the partition uuid
            uuid: None,
            // the partition partuuid
            part_uuid: Some(self.part_uuid.clone()),
            // the partition label
            part_label: if let Some(label) = self.volume.get_label() {
                Some(String::from(label))
            } else {
                None
            },
            // the partition size
            part_size: self.partition.get_size(),
        }
    }

    fn to_path_info(&self, path: &Path) -> Result<PathInfo, MigError> {
        Ok(PathInfo {
            // the physical device info
            device_info: self.to_device_info(),
            // the absolute path
            path: path.to_path_buf(),
            // the partition read only flag
            // pub mount_ro: bool,
            // The file system size
            fs_size: if let Some(capacity) = self.volume.get_capacity() {
                capacity
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!("No fs capacity found for path '{}'", path.display()),
                ));
            },
            // the fs free space
            fs_free: if let Some(free_space) = self.volume.get_free_space() {
                free_space
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!("No fs free_space found for path '{}'", path.display()),
                ));
            },
        })
    }
}

struct SharedDriveInfo {
    volumes: Option<Vec<VolumeInfo>>,
}

#[derive(Clone)]
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
                    inner: Arc::new(Mutex::new(SharedDriveInfo { volumes: None })),
                };

                // Put it in the heap so it can outlive this call
                DRIVE_INFO = transmute(Box::new(singleton));
            });

            (*DRIVE_INFO).clone()
        };

        let _dummy = drive_info.init()?;
        Ok(drive_info)
    }

    pub fn device_info_for_efi_drive(&self) -> Result<DeviceInfo, MigError> {
        let drive_info = self.init()?;
        if let Some(found) = drive_info
            .volumes
            .unwrap()
            .iter()
            .find(|vi| vi.volume.is_system())
        {
            Ok(found.to_device_info())
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                "Could not find EFI / System drive",
            ))
        }
    }

    pub fn path_info_from_path<P: AsRef<Path>>(&self, path: P) -> Result<PathInfo, MigError> {
        let drive_info = self.init()?;
        if let Some(found) = drive_info
            .volumes
            .unwrap()
            .iter()
            .find(|di| PathBuf::from(di.logical_drive.get_name()).starts_with(path.as_ref()))
        {
            Ok(found.to_path_info(path.as_ref())?)
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

        let efi_drive = if is_efi_boot()? {
            Some(mount_efi()?)
        } else {
            None
        };

        let mut vol_infos: Vec<VolumeInfo> = Vec::new();

        let part_uuid_re = Regex::new(VOL_UUID_REGEX).unwrap();

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
                            part_uuid: if let Some(captures) =
                                part_uuid_re.captures(volume.get_device_id())
                            {
                                String::from(captures.get(1).unwrap().as_str())
                            } else {
                                return Err(MigError::from_remark(
                                    MigErrorKind::NoMatch,
                                    &format!(
                                        "Could not extract partuuid from volume id: '{}'",
                                        volume.get_device_id()
                                    ),
                                ));
                            },
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
