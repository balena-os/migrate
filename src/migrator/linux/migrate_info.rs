use log::{debug, error, info, trace, warn};
use std::path::PathBuf;

use crate::{
    common::{
        balena_cfg_json::BalenaCfgJson,
        config::balena_config::{ImageType, PartDump},
        config::MigrateWifis,
        file_info::RelFileInfo,
        stage2_config::{CheckedFSDump, CheckedImageType, CheckedPartDump},
        wifi_config::WifiConfig,
        Config, FileInfo, FileType, MigError, MigErrorKind,
    },
    defs::OSArch,
    linux::{
        linux_common::{get_os_arch, get_os_name, to_std_device_path},
        EnsuredCmds,
    },
};

// *************************************************************************************************
// * Digested / Checked device-type independent properties from config and information retrieved
// * from device required for stage1 of migration
// *************************************************************************************************

pub(crate) mod lsblk_info;
pub(crate) use lsblk_info::{LsblkDevice, LsblkInfo, LsblkPartition};

pub(crate) mod path_info;
use crate::common::config::balena_config::FileRef;
pub(crate) use path_info::PathInfo;

//use crate::linux::migrate_info::lsblk_info::;

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

    pub image_file: CheckedImageType,
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

        let lsblk_info = LsblkInfo::all(&cmds)?;

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

        let os_image = match config.balena.get_image_path() {
            ImageType::Flasher(ref flasher_img) => {
                let checked_ref = MigrateInfo::check_file(
                    &flasher_img,
                    &FileType::GZipOSImage,
                    &work_path,
                    cmds,
                    &lsblk_info,
                )?;

                CheckedImageType::Flasher(checked_ref)
            }
            ImageType::FileSystems(ref fs_dump) => {
                // make sure all files are present and in /workdir, generate total size and partitioning config in miginfo
                CheckedImageType::FileSystems(CheckedFSDump {
                    device_slug: fs_dump.device_slug.clone(),
                    check: fs_dump.check.clone(),
                    max_data: fs_dump.max_data.clone(),
                    mkfs_direct: fs_dump.mkfs_direct.clone(),
                    extended_blocks: fs_dump.extended_blocks,
                    boot: CheckedPartDump {
                        archive: MigrateInfo::check_dump(
                            &fs_dump.boot,
                            &work_path,
                            cmds,
                            &lsblk_info,
                        )?,
                        blocks: fs_dump.boot.blocks,
                    },
                    root_a: CheckedPartDump {
                        archive: MigrateInfo::check_dump(
                            &fs_dump.root_a,
                            &work_path,
                            cmds,
                            &lsblk_info,
                        )?,
                        blocks: fs_dump.root_a.blocks,
                    },
                    root_b: CheckedPartDump {
                        archive: MigrateInfo::check_dump(
                            &fs_dump.root_b,
                            &work_path,
                            cmds,
                            &lsblk_info,
                        )?,
                        blocks: fs_dump.root_b.blocks,
                    },
                    state: CheckedPartDump {
                        archive: MigrateInfo::check_dump(
                            &fs_dump.state,
                            &work_path,
                            cmds,
                            &lsblk_info,
                        )?,
                        blocks: fs_dump.state.blocks,
                    },
                    data: CheckedPartDump {
                        archive: MigrateInfo::check_dump(
                            &fs_dump.data,
                            &work_path,
                            cmds,
                            &lsblk_info,
                        )?,
                        blocks: fs_dump.data.blocks,
                    },
                })
            }
        };

        let config_info = config.balena.get_config_path();
        let config_file = if let Some(file_info) = FileInfo::new(&config_info, &work_dir)? {
            // TODO: process digest
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
                balena_cfg.get_rel_path().display()
            );

            balena_cfg
        } else {
            error!("The balena config has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
            return Err(MigError::displayed());
        };

        let kernel_info = config.migrate.get_kernel_path();
        let kernel_file = if let Some(file_info) = FileInfo::new(&kernel_info, work_dir)? {
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

        let initrd_info = config.migrate.get_initrd_path();
        let initrd_file = if let Some(file_info) = FileInfo::new(&initrd_info, work_dir)? {
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

        let dtb_file = if let Some(dtb_path) = config.migrate.get_dtb_path() {
            if let Some(file_info) = FileInfo::new(&dtb_path, work_dir)? {
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
            if let Some(file_info) = FileInfo::new(&FileRef{path: file.clone() , hash: None}, &work_dir)? {
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
        let wifis: Vec<WifiConfig> = if MigrateWifis::None != wifi_cfg {
            // **********************************************************************
            // ** migrate wifi config
            // TODO: NetworkManager configs
            debug!("looking for wifi configurations to migrate");

            let empty_list: Vec<String> = Vec::new();
            let list: &Vec<String> = if let MigrateWifis::List(ref list) = wifi_cfg {
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
            image_file: os_image,
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

    fn check_dump(
        dump: &PartDump,
        work_path: &PathInfo,
        cmds: &EnsuredCmds,
        lsblk_info: &LsblkInfo,
    ) -> Result<RelFileInfo, MigError> {
        Ok(MigrateInfo::check_file(
            &dump.archive,
            &FileType::GZipTar,
            work_path,
            cmds,
            lsblk_info,
        )?)
    }

    fn check_file(
        file_ref: &FileRef,
        expected_type: &FileType,
        work_path: &PathInfo,
        cmds: &EnsuredCmds,
        lsblk_info: &LsblkInfo,
    ) -> Result<RelFileInfo, MigError> {
        if let Some(file_info) = FileInfo::new(&file_ref, &work_path.path)? {
            // make sure files are present and in /workdir, generate total size and partitioning config in miginfo
            let rel_path = if let Some(ref rel_path) = file_info.rel_path {
                rel_path.clone()
            } else {
                error!("The file '{}' was found outside of the working directory. This setup is not supported", file_ref.path.display());
                return Err(MigError::displayed());
            };

            let (_img_drive, img_part) = lsblk_info.get_path_info(&file_info.path)?;
            if img_part.get_path() != work_path.device {
                error!("The file '{}' appears to reside on a different partition from the working directory. This setup is not supported", file_ref.path.display());
                return Err(MigError::displayed());
            }

            // ensure expected type
            match file_info.expect_type(&cmds, expected_type) {
                Ok(_) => {
                    info!("The file '{}' image looks ok", file_info.path.display());
                }
                Err(_why) => {
                    // TODO: try gzip non compressed OS image
                    error!(
                        "The file '{}' does not match the expected type: '{:?}'",
                        file_ref.path.display(),
                        expected_type
                    );
                    return Err(MigError::displayed());
                }
            }

            Ok(RelFileInfo {
                rel_path,
                size: file_info.size,
                hash_info: file_info.hash_info,
            })
        } else {
            error!(
                "The balena file: '{}' can not be accessed.",
                file_ref.path.display()
            );
            return Err(MigError::displayed());
        }
    }
}
