use log::{info, debug, error};
use crate::{
    defs::{OSArch},
    common::{MigError, MigErrorKind, os_release::OSRelease, Config},
    mswin::{
        wmi_utils::{WmiUtils, LogicalDrive, PhysicalDrive},
        win_api::{is_efi_boot},
        util::{mount_efi},
        powershell::{PSInfo},
    },

};

pub(crate) struct MigrateInfo {
    pub os_name: String,
    pub os_arch: OSArch,
    pub os_release: OSRelease,
    pub boot_drive: Option<LogicalDrive>, 
}

impl MigrateInfo {
    pub fn new(_config: &Config, _ps_info: &mut PSInfo) -> Result<MigrateInfo, MigError> {
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

        let efi_boot = match is_efi_boot() {
            Ok(efi_boot) => {
                if efi_boot {
                    info!("The system is booted in EFI mode");
                    efi_boot
                } else {
                    error!("The system is booted in non EFI mode. Currently only EFI systems are supported on Windows");
                    return Err(MigError::displayed());
                }
            },
            Err(why) => {
                error!("Failed to determine EFI mode: {:?}", why);
                return Err(MigError::displayed());
            }
        };

        let mut boot_drive: Option<LogicalDrive> = None;
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
                                if partition.is_boot_device() {
                                    info!(
                                        "Boot partition is: '{}' type: '{}' on drive '{}'",
                                        partition.get_device(),
                                        partition.get_ptype(),
                                        drive.get_device_id()
                                    );

                                    install_drive = Some(drive.clone());

                                    boot_drive = match partition.query_logical_drive() {
                                        Ok(boot_drive) => {
                                            if let Some(boot_drive) = boot_drive {
                                                info!(
                                                    "Boot partition is mounted on: '{}' ",
                                                    boot_drive.get_name()
                                                );
                                                Some(boot_drive)
                                            } else {
                                                // TODO: mount it
                                                debug!("Boot partition is not mounted",);
                                                if efi_boot {
                                                    info!("Device was booted in EFI mode, attempting to mount the EFI partition");
                                                    let efi_drive = mount_efi()?;
                                                    info!(
                                                        "The EFI partition was mounted on '{}'",
                                                        efi_drive.get_name()
                                                    );
                                                    Some(efi_drive)
                                                } else {
                                                    error!(
                                                        "Failed to mount EFI partition for device"
                                                    );
                                                    return Err(MigError::displayed());
                                                }
                                            }
                                        }
                                        Err(why) => {
                                            error!("Failed to query logical drive for partition {}: {:?}", partition.get_device(), why);
                                            return Err(MigError::displayed());
                                        }
                                    };
                                } else {
                                    debug!("found partition: '{}'", partition.get_device());
                                }
                            }
                        }
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



        Ok(MigrateInfo{
            os_name: os_info.os_name,
            os_arch: os_info.os_arch,
            os_release: os_info.os_release,
            boot_drive: None,
        })
    }
}

