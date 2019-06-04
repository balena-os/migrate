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
use log::{debug, error, info};

pub(crate) struct MigrateInfo {
    pub os_name: String,
    pub os_arch: OSArch,
    pub os_release: OSRelease,
    pub boot_drive: Option<LogicalDrive>,
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

        let efi_vol = match MigrateInfo::get_efi_vol() {
            Ok(efi_vol) => {
                if !efi_vol.get_drive_letter().is_empty() {
                    info!("Found System/EFI volume on '{}', mounted on '{}'", efi_vol.get_device(), efi_vol.get_drive_letter());
                    efi_vol
                } else {
                    // attempt to mount it
                    info!("Found System/EFI volume on '{}', attempting to mount it", efi_vol.get_device());
                    match mount_efi() {
                        Ok(dl) => {
                            info!("The System/EFI Volume was mounted on '{}'", dl.get_name());
                            efi_vol
                        },
                        Err(why) => {
                            error!("Failed to mount EFI volume: {:?}", why);
                            return Err(MigError::displayed());
                        }
                    }
                }
            },
            Err(why) => {
               error!("Failed to query EFI volume: {:?}", why);
                return Err(MigError::displayed());
            }
        };

        let mut boot_vol = match Volume::query_boot_volumes() {
            Ok(volumes) => {
                if volumes.len() == 1 {
                    volumes[0].clone()
                } else {
                        error!("Encountered an unexpected number of boot volumes: {}", volumes.len());
                        return Err(MigError::displayed());
                }            },
            Err(why) => {
                error!("Failed to query boot volume: {:?}", why);
                return Err(MigError::displayed());
            }
        };

        let mut install_drive: Option<PhysicalDrive> = None;

        // Detect relevant drives
        // Detect boot partition and the drive it is on -> install drive
        // Attempt to guess linux names for drives partitions
        // -> InterfaceType SSI -> /dev/sda
        // -> InterfaceType IDE -> /dev/hda
        // -> InterfaceType ??SDCard?? -> /dev/mcblk

        match WmiUtils::query_drives() {
            Ok(phys_drives) => {
                for drive in phys_drives {
                    debug!(
                        "found drive id {}, device {}",
                        drive.get_device_id(),
                        drive.get_device()
                    );

                    let _partitions = match drive.query_partitions() {
                        Ok(partitions) => {
                            for partition in partitions {
                                if partition.get_device() == boot_vol.get_device() {
                                    info!(
                                        "Boot partition is: '{}' type: '{}' on drive '{}'",
                                        partition.get_device(),
                                        partition.get_ptype(),
                                        drive.get_device_id()
                                    );

                                    install_drive = Some(drive.clone());
                                    break;
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
                    };
                }
            }
            Err(why) => {
                error!("Failed to query drive info: {:?}", why);
                return Err(MigError::displayed());
            }
        }

        Ok(MigrateInfo {
            os_name: os_info.os_name,
            os_arch: os_info.os_arch,
            os_release: os_info.os_release,
            boot_drive: None,
        })
    }

    fn get_efi_vol() -> Result<Volume, MigError> {
        let volumes = Volume::query_system_volumes()?;
        if volumes.len() == 1 {
            debug!("Found system Volume: {:?}", volumes[0]);
            Ok(volumes[0].clone())
        } else {
            Err(MigError::from_remark(MigErrorKind::InvParam, &format!("Encountered an unexpected number of system volumes: {}", volumes.len())))
        }
    }

    fn get_efi_info(efi_vol: &Volume) -> Result<Volume, MigError> {

        let volumes = Volume::query_system_volumes()?;
        if volumes.len() == 1 {
            debug!("Found system Volume: {:?}", volumes[0]);
            Ok(volumes[0].clone())
        } else {
            Err(MigError::from_remark(MigErrorKind::InvParam, &format!("Encountered an unexpected number of system volumes: {}", volumes.len())))
        }
    }

}