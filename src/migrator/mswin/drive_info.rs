use crate::{
    common::{
        call, device_info::DeviceInfo, file_exists, path_append, path_info::PathInfo, MigError,
        MigErrorKind,
    },
    defs::{DISK_BY_LABEL_PATH, DISK_BY_PARTUUID_PATH},
    mswin::{
        util::mount_efi,
        win_api::{get_volume_disk_extents, is_efi_boot, DiskExtent},
        wmi_utils::{
            volume::{DriveType, Volume},
            LogicalDrive, Partition, PhysicalDrive,
        },
    },
};
use log::debug;
use regex::{Captures, Regex};
use std::mem::{swap, transmute};
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, Once};

// \\?\Volume{345ad334-48a8-11e8-9eaf-806e6f6e6963}\

const VOL_UUID_REGEX: &str =
    r#"^\\\\\?\\Volume\{([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})\}\\$"#;

#[derive(Clone)]
pub(crate) struct VolumeInfo {
    pub part_uuid: String,
    pub volume: Volume,
    pub logical_drive: LogicalDrive,
    pub physical_drive: PhysicalDrive,
    pub partition: Partition,
}

impl VolumeInfo {
    pub fn get_linux_path(&self) -> PathBuf {
        path_append(DISK_BY_PARTUUID_PATH, &self.part_uuid)
    }
}

#[derive(Clone)]
pub(crate) struct DriveInfo {
    volumes: Vec<VolumeInfo>,
}

impl DriveInfo {
    pub fn new() -> Result<DriveInfo, MigError> {
        let mut efi_drive = if is_efi_boot()? {
            Some(mount_efi()?)
        } else {
            None
        };

        let mut vol_infos: Vec<VolumeInfo> = Vec::new();

        let part_uuid_re = Regex::new(VOL_UUID_REGEX).unwrap();

        let volumes = Volume::query_all()?;
        for volume in volumes {
            match volume.get_drive_type() {
                DriveType::LocalDisk | DriveType::RemovableDisk => (),
                _ => {
                    debug!(
                        "Unsupported drive type: {:?} for volume '{}', skipping volume",
                        volume.get_drive_type(),
                        volume.get_device_id()
                    );
                    continue;
                }
            }

            let logical_drive = if let Some(drive_letter) = volume.get_drive_letter() {
                LogicalDrive::query_for_name(drive_letter)?
            } else {
                if volume.is_system() {
                    let mut swapped_efi_drive: Option<LogicalDrive> = None;
                    swap(&mut swapped_efi_drive, &mut efi_drive);
                    if let Some(efi_drive) = swapped_efi_drive {
                        efi_drive
                    } else {
                        debug!(
                            "No logicalDrive found for volume '{}' - skipping volume",
                            volume.get_device_id()
                        );
                        continue;
                    }
                } else {
                    debug!(
                        "No logicalDrive found for volume '{}' - skipping volume",
                        volume.get_device_id()
                    );
                    continue;
                }
            };

            // get DiskPartition for volume
            let disk_extents =
                get_volume_disk_extents(&format!("\\\\.\\{}", logical_drive.get_name()))?;

            if disk_extents.len() == 1 {
                let disk_extent = &disk_extents[0];
                let physical_drive = PhysicalDrive::by_index(disk_extent.disk_index as usize)?;
                if let Some(partition) = physical_drive
                    .query_partitions()?
                    .iter()
                    .find(|part| part.get_start_offset() == disk_extent.start_offset as u64)
                {
                    // got volume & partition here

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
                        logical_drive: logical_drive.clone(),
                        partition: partition.clone(),
                    });
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

        Ok(DriveInfo { volumes: vol_infos })
    }

    pub fn for_efi_drive(&self) -> Result<VolumeInfo, MigError> {
        if let Some(found) = self.volumes.iter().find(|vi| vi.volume.is_system()) {
            Ok(found.clone())
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                "Could not find EFI / System drive",
            ))
        }
    }

    pub fn from_path<'a, P: AsRef<Path>>(&'a self, path: P) -> Result<&'a VolumeInfo, MigError> {
        debug!("from_path: entered with '{}'", path.as_ref().display());
        let path = path.as_ref();
        if let Some(found) = self.volumes.iter().find(|di| {
            debug!("comparing to: '{}'", di.logical_drive.get_name());
            path.starts_with(di.logical_drive.get_name())
        }) {
            Ok(&found)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("No logical drive found for path '{}'", path.display()),
            ))
        }
    }

    pub fn from_partuuid<'a>(&'a self, partuuid: &str) -> Result<&'a VolumeInfo, MigError> {
        for vol_info in &self.volumes {
            if partuuid == vol_info.part_uuid {
                return Ok(vol_info);
            }
        }

        Err(MigError::from_remark(
            MigErrorKind::NotFound,
            &format!("No volume was found for partuuid: '{}' ", partuuid),
        ))
    }

    pub fn from_label<'a>(&'a self, label: &str) -> Result<&'a VolumeInfo, MigError> {
        for vol_info in &self.volumes {
            if let Some(label) = vol_info.volume.get_label() {
                return Ok(vol_info);
            }
        }

        Err(MigError::from_remark(
            MigErrorKind::NotFound,
            &format!("No volume was found for label: '{}' ", label),
        ))
    }
}
