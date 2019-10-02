use std::mem::transmute;
use std::sync::{Arc, Mutex, MutexGuard, Once};

use crate::common::{call, file_exists, path_append, MigErrCtx};
use crate::mswin::util::mount_efi;
use crate::mswin::win_api::is_efi_boot;
use crate::{
    common::{file_exists, path_append, MigError, MigErrorKind},
    mswin::{
        
        win_api::{get_volume_disk_extents, is_efi_boot, DiskExtent},
        wmi_utils::{LogicalDrive, Partition, PhysicalDrive, Volume},
    },
};

struct VolumeInfo {
    pub volume: Volume,
    pub logical_drive: LogicalDrive,
    pub physical_drive: PhysicalDrive,
    pub partition: Partition,
}

struct SharedDriveInfo {
    volumes: Option<Vec<VolumeInfo>>,
}

struct DriveInfo {
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
