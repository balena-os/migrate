use crate::{
    common::{os_release::OSRelease, Config, MigError, MigErrorKind},
    defs::OSArch,
    mswin::{
        powershell::PSInfo,
        util::mount_efi,
        win_api::is_efi_boot,
        wmi_utils::{LogicalDrive, PhysicalDrive, Volume, WmiUtils},
    },
};
use log::{debug, error, info, trace};

pub(crate) struct EfiDriveInfo{
    pub efi_vol: Volume,
    pub efi_mount: LogicalDrive,
    pub boot_vol: Volume,
    pub boot_mount: LogicalDrive,
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
                debug!("Found System/EFI Volume: {:?}", volumes[0]);
                volumes[0].clone()
            } else {
                return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("Encountered an unexpected number of system volumes: {}", volumes.len())));
            };

        let volumes = Volume::query_boot_volumes()?;
        let boot_vol =
            if volumes.len() == 1 {
                debug!("Found Boot Volume: {:?}", volumes[0]);
                volumes[0].clone()
            } else {
                return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("Encountered an unexpected number of Boot volumes: {}", volumes.len())))
            };

        let mut install_drive: Option<PhysicalDrive> = None;
        let mut boot_mount: Option<LogicalDrive> = None;
        let mut efi_mount: Option<LogicalDrive> = None;

        // get boot drive letter from boot volume flag
        // get efi drive from boot partition flag
        // mount efi drive if boot flagged partition is not mounted
        // ensure EFI partition

        /*
        match PhysicalDrive::query_all() {
            Ok(phys_drives) => {
                for drive in phys_drives {
                    debug!("found drive id {}, ", drive.get_device_id(), );
                    match drive.query_partitions() {
                        Ok(partitions) => {
                            for partition in partitions {
                                let part_dev = partition.get_device();
                                info!("Looking at partition: name: '{}' dev_id: '{}', device: {}, ", partition.get_name(), partition.get_device_id(), part_dev);

                                if part_dev == boot_vol.get_device() {
                                    info!(
                                        "Boot partition is: '{}' type: '{}' on drive '{}'",
                                        partition.get_device(),
                                        partition.get_ptype(),
                                        drive.get_device_id()
                                    );

                                    match partition.query_logical_drive()? {
                                        Some(log_drive) => {
                                            info!("Boot partition is: mounted on '{}'",log_drive.get_name());
                                            boot_mount = Some(log_drive.clone());
                                        },
                                        None => {
                                            error!(
                                                "Failed to retrieve logical drive for boot partition {}",
                                                part_dev,
                                            );
                                            return Err(MigError::displayed());
                                        },
                                    }
                                }

                                if part_dev == efi_vol.get_device() {
                                    info!(
                                        "System/EFI partition is: '{}' type: '{}' on drive '{}'",
                                        partition.get_device(),
                                        partition.get_ptype(),
                                        drive.get_device_id()
                                    );

                                    install_drive = Some(drive.clone());

                                    match partition.query_logical_drive()? {
                                        Some(log_drive) => {
                                            info!("System/EFI partition is: mounted on '{}'",log_drive.get_name());
                                            efi_mount = Some(log_drive.clone());
                                        },
                                        None => {
                                            info!("Attempting to mount System/EFI partition");
                                            match mount_efi() {
                                                Ok(log_drive) => {
                                                    info!("System/EFI partition {} is mounted on '{}'",part_dev, log_drive.get_name());
                                                    efi_mount = Some(log_drive.clone());
                                                },
                                                Err(why) => {
                                                    error!(
                                                        "Failed to retrieve mount logical drive for System/EFI partition {}",
                                                        part_dev,
                                                    );
                                                    return Err(MigError::displayed());
                                                }
                                            }
                                        },
                                    }
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
*/
        Ok(EfiDriveInfo{
            efi_vol,
            efi_mount:
                if let Some(efi_mount) = efi_mount {
                    efi_mount
                } else {
                    error!("No mounted System/EFI device was found");
                    return Err(MigError::displayed());
                },
            boot_vol,
            boot_mount:
                if let Some(boot_mount) = boot_mount {
                    boot_mount
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