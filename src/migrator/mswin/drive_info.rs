use crate::{
    common::{MigError, MigErrorKind},
    defs::{DISK_BY_LABEL_PATH, DISK_BY_PARTUUID_PATH},
    mswin::{
        util::mount_efi,
        win_api::{get_volume_disk_extents, is_efi_boot},
        wmi_utils::{
            volume::{DriveType, Volume},
            LogicalDisk, Partition, PhysicalDisk,
        },
    },
};
use log::{debug, warn};
use regex::Regex;
use std::mem::swap;
use std::path::Path;
use std::path::PathBuf;

// \\?\Volume{345ad334-48a8-11e8-9eaf-806e6f6e6963}\

const VOL_UUID_REGEX: &str =
    r#"^\\\\\?\\Volume\{([a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12})\}\\$"#;

#[derive(Clone)]
pub(crate) struct VolumeInfo {
    pub part_uuid: String,
    pub volume: Volume,
    pub logical_drive: LogicalDisk,
    pub physical_drive: PhysicalDisk,
    pub partition: Partition,
}

impl VolumeInfo {
/*
    pub fn get_linux_path(&self) -> PathBuf {
        if self.partition.is_gpt_partition() {
            PathBuf::from(&format!("{}/{}", DISK_BY_PARTUUID_PATH, &self.part_uuid))
        } else {
            if let Some(label) = self.volume.get_label() {
                PathBuf::from(&format!("{}/{}", DISK_BY_LABEL_PATH, label))
            } else {
                PathBuf::from(&format!("{}/{}", DISK_BY_PARTUUID_PATH, &self.part_uuid))
            }
        }
    }
*/
}

#[derive(Clone)]
pub(crate) struct DriveInfo {
    volumes: Vec<VolumeInfo>,
}

impl DriveInfo {
    pub fn new() -> Result<DriveInfo, MigError> {
        debug!("new: entered");
        let mut found_efi_drive = if is_efi_boot()? {
            debug!("attempting to mount/locate efi drive");
            Some(mount_efi()?)
        } else {
            None
        };

        let mut vol_infos: Vec<VolumeInfo> = Vec::new();

        let part_uuid_re = Regex::new(VOL_UUID_REGEX).unwrap();
        debug!("new: ******************************** scanning volumes");
        let volumes = Volume::query_all()?;
        for volume in volumes {
            debug!("new: ***** looking at volume {}", volume);
            match volume.get_drive_type() {
                DriveType::LocalDisk | DriveType::RemovableDisk => (),
                _ => {
                    debug!(
                        "new: Unsupported drive type: {:?} for volume '{}', skipping volume",
                        volume.get_drive_type(),
                        volume.get_device_id()
                    );
                    continue;
                }
            }

            let logical_disk = if let Some(drive_letter) = volume.get_drive_letter() {
                // find a LogicalDisk for this volume
                LogicalDisk::query_for_name(drive_letter)?
            } else {
                if volume.is_system() {
                    debug!("new: failed to match logical disk by drive letter, trying EFI drive");
                    // volume is the EFI volume - use found_efi_drive as this volumes
                    // logical drive
                    let mut swapped_efi_drive: Option<LogicalDisk> = None;
                    swap(&mut swapped_efi_drive, &mut found_efi_drive);
                    if let Some(efi_drive) = swapped_efi_drive {
                        efi_drive
                    } else {
                        warn!(
                            "No logicalDrive found for system volume '{}' - skipping volume",
                            volume.get_device_id()
                        );
                        continue;
                    }
                } else {
                    warn!(
                        "No logicalDrive found for volume '{}' - skipping volume",
                        volume.get_device_id()
                    );
                    continue;
                }
            };

            debug!("new: got logical_disk_for volume: {}", logical_disk);

            // get DiskPartition for volume
            let disk_extents =
                get_volume_disk_extents(&format!("\\\\.\\{}", logical_disk.get_name()))?;

            if let Some(disk_extent) = disk_extents.get(0) {
                let physical_disk = PhysicalDisk::by_index(disk_extent.disk_index as usize)?;
                if let Some(partition) = physical_disk
                    .query_partitions()?
                    .iter()
                    .find(|part| part.get_start_offset() == disk_extent.start_offset as u64)
                {
                    // got volume, logical_disk, physical_disk & partition here
                    debug!("new: got physical disk_for volume: {}", physical_disk);
                    debug!("new: got partition disk_for volume: {}", partition);
                    // try to extract a partuuid from volume_id
                    let part_uuid = if let Some(captures) =
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
                    };

                    debug!("new: found partuuid: {} for volume {}", part_uuid, volume);

                    // Her we have all components: PhysicalDrive, Volume, Partition, LogicalDrive
                    vol_infos.push(VolumeInfo {
                        part_uuid,
                        volume,
                        physical_drive: physical_disk,
                        logical_drive: logical_disk.clone(),
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
                        "No disk extents found for for volume '{}'",
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
            if vol_info.volume.get_label().is_some() {
                return Ok(vol_info);
            }
        }

        Err(MigError::from_remark(
            MigErrorKind::NotFound,
            &format!("No volume was found for label: '{}' ", label),
        ))
    }
}
