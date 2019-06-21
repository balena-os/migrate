use log::{debug, error, info, trace, warn};
use std::path::PathBuf;

use crate::{
    common::{
        balena_cfg_json::BalenaCfgJson, wifi_config::WifiConfig, Config, FileInfo, FileType,
        MigError, MigErrorKind, MigrateWifis,
    },
    defs::OSArch,
    linux::{
        linux_common::{get_os_arch, get_os_name},
        EnsuredCmds,
    },
};

// *************************************************************************************************
// * Digested / Checked device-type independent properties from config and information retrieved
// * from device required for stage1 of migration
// *************************************************************************************************

pub(crate) mod lsblk_info;
pub(crate) use lsblk_info::LsblkInfo;

pub(crate) mod label_type;

pub(crate) mod path_info;
use crate::linux::linux_common::to_std_device_path;
use crate::linux::migrate_info::lsblk_info::{LsblkDevice, LsblkPartition};
pub(crate) use path_info::PathInfo;

#[derive(Debug)]
pub(crate) struct MigrateInfo {
    pub os_name: String,
    pub os_arch: OSArch,

    pub lsblk_info: LsblkInfo,
    // pub root_path: PathInfo,
    // pub boot_path: PathInfo,
    pub work_path: PathInfo,
    pub log_path: Option<(PathBuf, LsblkDevice, LsblkPartition)>,

    pub nwmgr_files: Vec<FileInfo>,
    pub wifis: Vec<WifiConfig>,

    pub image_file: FileInfo,
    pub config_file: BalenaCfgJson,
    pub kernel_file: FileInfo,
    pub initrd_file: FileInfo,
    pub dtb_file: Option<FileInfo>,
}

// TODO: /etc path just in case
// TODO: sort out error reporting with Displayed

impl MigrateInfo {
    pub(crate) fn new(config: &Config, cmds: &mut EnsuredCmds) -> Result<MigrateInfo, MigError> {
        trace!("new: entered");
        // TODO: check files configured in config & create file_infos

        let os_arch = get_os_arch(&cmds)?;

        let lsblk_info = LsblkInfo::new(&cmds)?;

        let work_path = if let Some(path_info) =
            PathInfo::new(&cmds, config.migrate.get_work_dir(), &lsblk_info)?
        {
            path_info
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "the device for path '{}' could not be established",
                    config.migrate.get_work_dir().display()
                ),
            ));
        };

        let log_path = if let Some(log_dev) = config.migrate.get_log_device() {
            if let Ok(ref std_dev) = to_std_device_path(log_dev) {
                if let Ok((log_drive, log_part)) = lsblk_info.get_devinfo_from_partition(std_dev) {
                    if let Some(ref fstype) = log_part.fstype {
                        info!(
                            "Found log device '{}' with file system type '{}'",
                            log_dev.display(),
                            fstype
                        );
                        Some((PathBuf::from(log_dev), log_drive.clone(), log_part.clone()))
                    } else {
                        warn!("Could not determine file system type for log partition '{}'  - ignoring", log_dev.display());
                        None
                    }
                } else {
                    warn!(
                        "failed to find lsblk info for log device '{}'",
                        log_dev.display()
                    );
                    None
                }
            } else {
                warn!("failed to evaluate log device '{}'", log_dev.display());
                None
            }
        } else {
            None
        };

        let work_dir = &work_path.path;
        info!("Working directory is '{}'", work_dir.display());

        let image_file = if let Some(file_info) =
            FileInfo::new(&config.balena.get_image_path(), &work_dir)?
        {
            // Make sure balena image is in workdir and on its mount

            if let None = file_info.rel_path {
                error!("The balena OS image was found outside of the working directory. This setup is not supported");
                return Err(MigError::displayed());
            }

            let (_img_drive, img_part) = lsblk_info.get_path_info(&file_info.path)?;
            if img_part.get_path() != work_path.device {
                error!("The balena OS image appears to reside on a different partition from the working directory. This setup is not supported");
                return Err(MigError::displayed());
            }

            // ensure expected type
            match file_info.expect_type(&cmds, &FileType::OSImage) {
                Ok(_) => {
                    info!(
                        "The balena OS image looks ok: '{}'",
                        file_info.path.display()
                    );
                }
                Err(_why) => {
                    error!(
                        "The balena OS image does not match the expected type: '{:?}'",
                        FileType::OSImage
                    );
                    return Err(MigError::displayed());
                }
            }

            /*            // TODO: do this later, flash device will be determined by device / bootmanager
                        let required_mem = file_info.size + APPROX_MEM_THRESHOLD;
                        if get_mem_info()?.0 < required_mem {
                            error!("We have not found sufficient memory to store the balena OS image in ram. at least {} of memory is required.", format_size_with_unit(required_mem));
                            return Err(MigError::from(MigErrorKind::Displayed));
                        }
            */
            file_info
        } else {
            error!("The balena image has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
            return Err(MigError::displayed());
        };

        let config_file = if let Some(file_info) =
            FileInfo::new(&config.balena.get_config_path(), &work_dir)?
        {
            // Make sure balena config is in workdir and on its mount
            if let None = file_info.rel_path {
                error!("The balena OS config was found outside of the working directory. This setup is not supported");
                return Err(MigError::displayed());
            }

            let (_cfg_drive, cfg_part) = lsblk_info.get_path_info(&file_info.path)?;
            if cfg_part.get_path() != work_path.device {
                error!("The balena OS config appears to reside on a different partition from the working directory. This setup is not supported");
                return Err(MigError::displayed());
            }

            // ensure expected type
            match file_info.expect_type(&cmds, &FileType::Json) {
                Ok(_) => (),
                Err(_why) => {
                    error!(
                        "The balena OS config does not match the expected type: '{:?}'",
                        FileType::Json
                    );
                    return Err(MigError::displayed());
                }
            }

            // check config
            let balena_cfg = BalenaCfgJson::new(file_info)?;
            info!(
                "The balena config file looks ok: '{}'",
                balena_cfg.get_path().display()
            );

            balena_cfg
        } else {
            error!("The balena image has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
            return Err(MigError::displayed());
        };

        let kernel_file = if let Some(file_info) =
            FileInfo::new(config.migrate.get_kernel_path(), work_dir)?
        {
            file_info.expect_type(
                &cmds,
                match os_arch {
                    OSArch::AMD64 => &FileType::KernelAMD64,
                    OSArch::ARMHF => &FileType::KernelARMHF,
                    OSArch::I386 => &FileType::KernelI386,
                },
            )?;

            info!(
                "The balena migrate kernel looks ok: '{}'",
                file_info.path.display()
            );
            file_info
        } else {
            error!("The migrate kernel has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
            return Err(MigError::displayed());
        };

        let initrd_file = if let Some(file_info) =
            FileInfo::new(config.migrate.get_initrd_path(), work_dir)?
        {
            file_info.expect_type(&cmds, &FileType::InitRD)?;
            info!(
                "The balena migrate initramfs looks ok: '{}'",
                file_info.path.display()
            );
            file_info
        } else {
            error!("The migrate initramfs has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
            return Err(MigError::displayed());
        };

        let dtb_file = if let Some(ref dtb_path) = config.migrate.get_dtb_path() {
            if let Some(file_info) = FileInfo::new(dtb_path, work_dir)? {
                file_info.expect_type(&cmds, &FileType::DTB)?;
                info!(
                    "The balena migrate device tree blob looks ok: '{}'",
                    file_info.path.display()
                );
                Some(file_info)
            } else {
                error!("The migrate device tree blob has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
                return Err(MigError::displayed());
            }
        } else {
            None
        };

        let mut nwmgr_files: Vec<FileInfo> = Vec::new();

        for file in config.migrate.get_nwmgr_files() {
            if let Some(file_info) = FileInfo::new(&file, &work_dir)? {
                file_info.expect_type(&cmds, &FileType::Text)?;
                info!(
                    "Adding network manager config: '{}'",
                    file_info.path.display()
                );
                nwmgr_files.push(file_info);
            } else {
                error!(
                    "The network manager config file '{}' could not be found",
                    file.display()
                );
                return Err(MigError::displayed());
            }
        }

        let wifi_cfg = config.migrate.get_wifis();
        let wifis: Vec<WifiConfig> = if MigrateWifis::NONE != wifi_cfg {
            // **********************************************************************
            // ** migrate wifi config
            // TODO: NetworkManager configs
            debug!("looking for wifi configurations to migrate");

            let empty_list: Vec<String> = Vec::new();
            let list: &Vec<String> = if let MigrateWifis::SOME(ref list) = wifi_cfg {
                list
            } else {
                &empty_list
            };

            let wifi_list = WifiConfig::scan(list)?;

            if wifi_list.len() > 0 {
                for wifi in &wifi_list {
                    info!("Found config for wifi: {}", wifi.get_ssid());
                }
                wifi_list
            } else {
                info!("No wifi configurations found");
                Vec::new()
            }
        } else {
            Vec::new()
        };

        if nwmgr_files.is_empty() && wifis.is_empty() {
            if config.migrate.require_nwmgr_configs() {
                error!("No Network manager files were found, the device might not be able to come online");
                return Err(MigError::from(MigErrorKind::Displayed));
            }
        }

        let result = MigrateInfo {
            os_name: get_os_name()?,
            os_arch,
            lsblk_info,
            work_path,
            log_path,
            image_file,
            kernel_file,
            initrd_file,
            dtb_file,
            nwmgr_files,
            config_file,
            wifis,
        };

        debug!("Diskinfo: {:?}", result);

        Ok(result)
    }
}
