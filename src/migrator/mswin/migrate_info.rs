use crate::{
    common::{os_release::OSRelease, Config, MigError, MigErrorKind},
    defs::OSArch,
    mswin::{
        powershell::PSInfo,
        util::mount_efi,
        win_api::is_efi_boot,
        wmi_utils::{LogicalDrive, PhysicalDrive, Volume, Partition, WmiUtils},
    },
};
use log::{debug, error, info, trace};

pub(crate) struct EfiDriveInfo{
    pub efi_vol: Volume,
    pub efi_partition: Partition,
    pub efi_mount: LogicalDrive,
    pub boot_vol: Volume,
    pub boot_partition: Partition,
    pub drive: PhysicalDrive,
}


pub(crate) struct MigrateInfo {
    pub os_name: String,
    pub os_arch: OSArch,
    pub os_release: OSRelease,
    pub drive_info: EfiDriveInfo,
}

impl MigrateInfo {
    pub fn new(_config: &Config, _ps_info: &mut PSInfo) -> Result<MigrateInfo, MigError> {
        trace!("new: entered");

        let efi_boot = match is_efi_boot() {
            Ok(efi_boot) => {
                if efi_boot {
                    info!("The system is booted in EFI mode");
                    efi_boot
                } else {
                    error!("The system is booted in non EFI mode. Currently only EFI systems are supported on Windows");
                    return Err(MigError::displayed());
                }
            }
            Err(why) => {
                error!("Failed to determine EFI mode: {:?}", why);
                return Err(MigError::displayed());
            }
        };

        debug!("get_os_info():");
        
        let os_info = match WmiUtils::get_os_info() {
            Ok(os_info) => {
                info!(
                    "OS Architecture is {}, OS Name is '{}', OS Release is '{}'",
                    os_info.os_arch, os_info.os_name, os_info.os_release
                );
                debug!("Boot device: '{}'", os_info.boot_dev);
                os_info
            }
            Err(why) => {
                error!("Failed to retrieve OS info: {:?}", why);
                return Err(MigError::displayed());
            }
        };


        let efi_drive_info = MigrateInfo::get_efi_drive_info()?;

        // Detect relevant drives
        // Detect boot partition and the drive it is on -> install drive
        // Attempt to guess linux names for drives partitions
        // -> InterfaceType SSI -> /dev/sda
        // -> InterfaceType IDE -> /dev/hda
        // -> InterfaceType ??SDCard?? -> /dev/mcblk


        Ok(MigrateInfo {
            os_name: os_info.os_name,
            os_arch: os_info.os_arch,
            os_release: os_info.os_release,
            drive_info: efi_drive_info,
        })
    }

    fn get_efi_drive_info() -> Result<EfiDriveInfo, MigError> {
        trace!("get_efi_drive_info: entered");
        // get the system/EFI volume
        let volumes = Volume::query_system_volumes()?;
        let efi_vol =
            if volumes.len() == 1 {
                debug!("Found System/EFI Volume: '{}' dev_id: '{}'", volumes[0].get_name(), volumes[0].get_device_id());
                volumes[0].clone()
            } else {
                return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("Encountered an unexpected number of system volumes: {}", volumes.len())));
            };

        let volumes = Volume::query_boot_volumes()?;
        let boot_vol =
            if volumes.len() == 1 {
                debug!("Found Boot Volume: '{}' dev_id: '{}'", volumes[0].get_name(), volumes[0].get_device_id());
                volumes[0].clone()
            } else {
                return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("Encountered an unexpected number of Boot volumes: {}", volumes.len())))
            };

        let mut install_drive: Option<PhysicalDrive> = None;
        let mut boot_partition: Option<Partition> = None;
        let mut efi_partition: Option<Partition> = None;
        let mut efi_mount: Option<LogicalDrive> = None;

        // get boot drive letter from boot volume flag
        // get efi drive from boot partition flag
        // mount efi drive if boot flagged partition is not mounted
        // ensure EFI partition

        match PhysicalDrive::query_all() {
            Ok(phys_drives) => {
                for drive in phys_drives {
                    debug!("found drive id {}, ", drive.get_device_id(), );
                    match drive.query_partitions() {
                        Ok(partitions) => {
                            for partition in partitions {                                
                                debug!("Looking at partition: name: '{}' dev_id: '{}'", partition.get_name(), partition.get_device_id());
                                if let Some(logical_drive) = partition.query_logical_drive()? {
                                    if logical_drive.get_name() == boot_vol.get_drive_letter() {
                                        info!("Found boot drive on partition '{}' , drive: '{}'", partition.get_device_id(), drive.get_device_id());
                                        install_drive = Some(drive.clone());
                                        boot_partition = Some(partition.clone());
                                    }
                                }

                                if partition.is_boot_device() {
                                    info!("Found potential System/EFI drive on partition '{}' , drive: '{}'", partition.get_device_id(), drive.get_device_id());            
                                    if let Some(logical_drive) = partition.query_logical_drive()? {
                                        info!("System/EFI drive is mounted on  '{}'", logical_drive.get_name());                                      
                                        efi_partition = Some(partition.clone());
                                        efi_mount = Some(logical_drive);
                                    } else {
                                        info!("Attempting to mount System/EFI drive");                                      
                                        match mount_efi() {
                                            Ok(logical_drive) => {                      
                                                info!("System/EFI drive was mounted on  '{}'", logical_drive.get_name());                                      
                                                efi_partition = Some(partition.clone());
                                                efi_mount = Some(logical_drive);
                                            },
                                            Err(why) => {
                                                error!(
                                                    "Failed to retrieve logical drive for efi partition {}",
                                                    partition.get_device_id(),
                                                );
                                                return Err(MigError::displayed());
                                            }
                                        }
                                    }
                                    info!("Found System/EFI drive on partition '{}' , drive: '{}'", partition.get_device_id(), drive.get_device_id());
                                }
                            }
                        },
                        Err(why) => {
                            error!(
                                "Failed to query partitions for drive {}: {:?}",
                                drive.get_device_id(),
                                why
                            );
                            return Err(MigError::displayed());
                        }
                    }
                }
            }
            Err(why) => {
                error!("Failed to query drive info: {:?}", why);
                return Err(MigError::displayed());
            }
        }

        Ok(EfiDriveInfo{
            efi_vol,
            efi_partition:
                if let Some(efi_partition) = efi_partition {
                    efi_partition
                } else {
                    error!("No mounted System/EFI device was found");
                    return Err(MigError::displayed());
                },
            efi_mount:
                if let Some(efi_mount) = efi_mount {
                    efi_mount
                } else {
                    error!("No mounted System/EFI device was found");
                    return Err(MigError::displayed());
                },
            boot_vol,
            boot_partition:
                if let Some(boot_partition) = boot_partition {
                    boot_partition
                } else {
                    error!("No mounted Boot device was found");
                    return Err(MigError::displayed());
                },
            drive:
                if let Some(drive) = install_drive {
                    drive
                } else {
                    error!("No install drive was found");
                    return Err(MigError::displayed());
                },
        })
    }

}