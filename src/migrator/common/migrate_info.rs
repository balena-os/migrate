use log::{debug, error, info, trace, warn};

use crate::{
    common::{
        config::{
            balena_config::{FileRef, ImageType, PartDump},
            migrate_config::MigrateWifis,
        },
        device_info::DeviceInfo,
        file_info::RelFileInfo,
        os_api::OSApi,
        path_info::PathInfo,
        stage2_config::{CheckedFSDump, CheckedImageType, CheckedPartDump},
        wifi_config::WifiConfig,
        Config, FileInfo, MigError, MigErrorKind,
    },
    defs::FileType,
    defs::OSArch,
};

// *************************************************************************************************
// * Digested / Checked device-type independent properties from config and information retrieved
// * from device required for stage1 of migration
// *************************************************************************************************

pub(crate) mod balena_cfg_json;
pub(crate) use balena_cfg_json::BalenaCfgJson;

//use crate::linux::migrate_info::lsblk_info::;

#[derive(Debug)]
pub(crate) struct MigrateInfo {
    pub os_name: String,
    pub os_arch: OSArch,

    pub work_path: PathInfo,
    pub log_path: Option<DeviceInfo>,

    pub nwmgr_files: Vec<FileInfo>,
    pub wifis: Vec<WifiConfig>,

    pub image_file: CheckedImageType,
    pub config_file: BalenaCfgJson,

    pub kernel_file: FileInfo,

    pub initrd_file: FileInfo,

    pub dtb_file: Vec<FileInfo>,
}

// TODO: sort out error reporting with Displayed

impl MigrateInfo {
    pub(crate) fn new(config: &Config, os_api: &impl OSApi) -> Result<MigrateInfo, MigError> {
        trace!("new: entered");
        let os_arch = os_api.get_os_arch()?;

        debug!(
            "Calling PathInfo::from_path on '{}'",
            config.migrate.get_work_dir().display()
        );

        let work_path = PathInfo::from_path(config.migrate.get_work_dir())?;
        let work_dir = &work_path.path;
        info!(
            "Working directory is '{}' on '{}'",
            work_dir.display(),
            work_path.device_info.drive
        );

        let log_path = if let Some(log_dev) = config.migrate.get_log_device() {
            debug!("Checking log device: '{}'", log_dev.display());
            if log_dev.exists() {
                Some(os_api.device_info_from_partition(log_dev)?)
            } else {
                warn!(
                    "Configured log drive '{}' could not be found",
                    log_dev.display()
                );
                None
            }
        } else {
            None
        };

        debug!("Checking image files: {:?}", config.balena.get_image_path());

        let os_image = match config.balena.get_image_path() {
            ImageType::Flasher(ref flasher_img) => {
                let checked_ref = MigrateInfo::check_file(
                    &flasher_img,
                    &FileType::GZipOSImage,
                    &work_path,
                    os_api,
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
                        archive: MigrateInfo::check_dump(&fs_dump.boot, &work_path, os_api)?,
                        blocks: fs_dump.boot.blocks,
                    },
                    root_a: CheckedPartDump {
                        archive: MigrateInfo::check_dump(&fs_dump.root_a, &work_path, os_api)?,
                        blocks: fs_dump.root_a.blocks,
                    },
                    root_b: CheckedPartDump {
                        archive: MigrateInfo::check_dump(&fs_dump.root_b, &work_path, os_api)?,
                        blocks: fs_dump.root_b.blocks,
                    },
                    state: CheckedPartDump {
                        archive: MigrateInfo::check_dump(&fs_dump.state, &work_path, os_api)?,
                        blocks: fs_dump.state.blocks,
                    },
                    data: CheckedPartDump {
                        archive: MigrateInfo::check_dump(&fs_dump.data, &work_path, os_api)?,
                        blocks: fs_dump.data.blocks,
                    },
                })
            }
        };

        debug!(
            "Checking config.json: '{:?}'",
            config.balena.get_config_path()
        );
        let config_file = if let Some(file_info) =
            FileInfo::new(config.balena.get_config_path(), &work_dir)?
        {
            if let None = file_info.rel_path {
                error!("The balena OS config was found outside of the working directory. This setup is not supported");
                return Err(MigError::displayed());
            }

            let cfg_path_info = os_api.path_info_from_path(&file_info.path)?;
            if cfg_path_info.device_info.mountpoint != work_path.device_info.mountpoint {
                error!("The balena OS config appears to reside on a different partition from the working directory. This setup is not supported");
                return Err(MigError::displayed());
            }

            // ensure expected type
            match os_api.expect_type(&file_info.path, &FileType::Json) {
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
            //balena_cfg.check()
            balena_cfg
        } else {
            error!("The balena config has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
            return Err(MigError::displayed());
        };

        let kernel_info = config.migrate.get_kernel_path();

        let kernel_file = if let Some(file_info) = FileInfo::new(&kernel_info, work_dir)? {
            // TODO: check later, when target arch is known
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
            os_api.expect_type(&file_info.path, &FileType::InitRD)?;
            info!(
                "The balena migrate initramfs looks ok: '{}'",
                file_info.path.display()
            );
            file_info
        } else {
            error!("The migrate initramfs has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
            return Err(MigError::displayed());
        };

        let dtb_files = if let Some(dtb_refs) = config.migrate.get_dtb_refs() {
            let mut dtb_files: Vec<FileInfo> = Vec::new();
            for dtb_ref in dtb_refs {
                if let Some(file_info) = FileInfo::new(dtb_ref, work_dir)? {
                    os_api.expect_type(&file_info.path, &FileType::DTB)?;
                    info!(
                        "The balena migrate device tree blob looks ok: '{}'",
                        file_info.path.display()
                    );
                    dtb_files.push(file_info);
                } else {
                    error!("The migrate device tree blob '{}' cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files", dtb_ref.path.display());
                    return Err(MigError::displayed());
                }
            }
            dtb_files
        } else {
            Vec::new()
        };

        let mut nwmgr_files: Vec<FileInfo> = Vec::new();

        for file in config.migrate.get_nwmgr_files() {
            if let Some(file_info) = FileInfo::new(
                &FileRef {
                    path: file.clone(),
                    hash: None,
                },
                &work_dir,
            )? {
                os_api.expect_type(&file_info.path, &FileType::Text)?;
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
            os_name: os_api.get_os_name()?,
            os_arch,
            work_path,
            log_path,
            image_file: os_image,
            kernel_file,
            initrd_file,
            dtb_file: dtb_files,
            nwmgr_files,
            config_file,
            wifis,
        };

        debug!("MigrateInfo: {:?}", result);

        Ok(result)
    }

    fn check_dump(
        dump: &PartDump,
        work_path: &PathInfo,
        os_api: &impl OSApi,
    ) -> Result<RelFileInfo, MigError> {
        Ok(MigrateInfo::check_file(
            &dump.archive,
            &FileType::GZipTar,
            work_path,
            os_api,
        )?)
    }

    fn check_file(
        file_ref: &FileRef,
        expected_type: &FileType,
        work_path: &PathInfo,
        os_api: &impl OSApi,
    ) -> Result<RelFileInfo, MigError> {
        if let Some(file_info) = FileInfo::new(&file_ref, &work_path.path)? {
            // make sure files are present and in /workdir, generate total size and partitioning config in miginfo
            let rel_path = if let Some(ref rel_path) = file_info.rel_path {
                rel_path.clone()
            } else {
                error!("The file '{}' was found outside of the working directory. This setup is not supported", file_ref.path.display());
                return Err(MigError::displayed());
            };

            let file_path_info = os_api.path_info_from_path(&file_info.path)?;
            if file_path_info.device_info.mountpoint != work_path.device_info.mountpoint {
                error!("The file '{}' appears to reside on a different partition from the working directory. This setup is not supported", file_ref.path.display());
                return Err(MigError::displayed());
            }

            // ensure expected type
            match os_api.expect_type(&file_info.path, expected_type) {
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
