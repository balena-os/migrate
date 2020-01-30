use log::{debug, error, info, warn};

use crate::{
    common::{
        config::{
            balena_config::{FileRef, ImageType, PartDump},
            migrate_config::MigrateWifis,
        },
        file_info::RelFileInfo,
        os_api::{OSApi, OSApiImpl},
        path_info::PathInfo,
        stage2_config::{CheckedFSDump, CheckedImageType, CheckedPartDump},
        wifi_config::WifiConfig,
        Config, FileInfo, MigError, MigErrorKind,
    },
    defs::{FileType, OSArch},
};

// *************************************************************************************************
// * migrate_info holds digested and checked device-type independent properties from config and
// information retrieved from device required for stage1 of migration
// *************************************************************************************************

pub(crate) mod balena_cfg_json;
pub(crate) use balena_cfg_json::BalenaCfgJson;
use std::path::PathBuf;

#[derive(Debug)]
pub(crate) struct MigrateInfo {
    pub os_name: String,
    pub os_arch: OSArch,

    pub work_path: PathInfo,
    pub log_path: Option<PathBuf>,

    pub nwmgr_files: Vec<FileInfo>,
    pub wifis: Vec<WifiConfig>,

    pub image_file: CheckedImageType,
    pub config_file: BalenaCfgJson,

    pub kernel_file: FileInfo,

    pub initrd_file: FileInfo,
    //pub dtb_file: Vec<FileInfo>,
}

// TODO: sort out error reporting with Displayed

impl MigrateInfo {
    #[allow(clippy::cognitive_complexity)] //TODO refactor this function to fix the clippy warning
    pub(crate) fn new(config: &Config) -> Result<MigrateInfo, MigError> {
        debug!("new: entered");
        let os_api = OSApiImpl::new()?;
        let os_arch = os_api.get_os_arch()?;
        let work_path = os_api.path_info_from_path(config.migrate.get_work_dir())?;
        let work_dir = &work_path.path;

        info!(
            "Working directory is '{}' on '{}'",
            work_dir.display(),
            work_path.device_info.device
        );

        let log_path = if let Some(log_dev) = config.migrate.get_log_device() {
            debug!("Checking log device: '{:?}'", log_dev);
            match os_api.device_path_from_partition(log_dev) {
                Ok(dev_info) => {
                    info!("Using log path: '{}'", dev_info.display());
                    Some(dev_info)
                }
                Err(why) => {
                    warn!(
                        "Unable to determine log device: {:?}, error: {:?}",
                        log_dev, why
                    );
                    None
                }
            }
        } else {
            None
        };

        debug!("Checking image files: {:?}", config.balena.get_image_path());

        let os_image = match config.balena.get_image_path() {
            ImageType::Flasher(ref flasher_img) => {
                let checked_ref =
                    MigrateInfo::check_file(&flasher_img, &FileType::GZipOSImage, &work_path)?;

                CheckedImageType::Flasher(checked_ref)
            }
            ImageType::FileSystems(ref fs_dump) => {
                // make sure all files are present and in workdir
                CheckedImageType::FileSystems(CheckedFSDump {
                    device_slug: fs_dump.device_slug.clone(),
                    check: fs_dump.check.clone(),
                    max_data: fs_dump.max_data,
                    mkfs_direct: fs_dump.mkfs_direct,
                    extended_blocks: fs_dump.extended_blocks,
                    boot: CheckedPartDump {
                        archive: MigrateInfo::check_dump(&fs_dump.boot, &work_path)?,
                        blocks: fs_dump.boot.blocks,
                    },
                    root_a: CheckedPartDump {
                        archive: MigrateInfo::check_dump(&fs_dump.root_a, &work_path)?,
                        blocks: fs_dump.root_a.blocks,
                    },
                    root_b: CheckedPartDump {
                        archive: MigrateInfo::check_dump(&fs_dump.root_b, &work_path)?,
                        blocks: fs_dump.root_b.blocks,
                    },
                    state: CheckedPartDump {
                        archive: MigrateInfo::check_dump(&fs_dump.state, &work_path)?,
                        blocks: fs_dump.state.blocks,
                    },
                    data: CheckedPartDump {
                        archive: MigrateInfo::check_dump(&fs_dump.data, &work_path)?,
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
            if file_info.rel_path.is_none() {
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

            // check config, balena_cfg_json::check is done later when device info is present
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

            if !wifi_list.is_empty() {
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

        if nwmgr_files.is_empty() && wifis.is_empty() && config.migrate.require_nwmgr_configs() {
            error!(
                "No Network manager files were found, the device might not be able to come online"
            );
            return Err(MigError::from(MigErrorKind::Displayed));
        }

        let result = MigrateInfo {
            os_name: os_api.get_os_name()?,
            os_arch,
            work_path,
            log_path,
            image_file: os_image,
            kernel_file,
            initrd_file,
            nwmgr_files,
            config_file,
            wifis,
        };

        debug!("MigrateInfo: {:?}", result);

        Ok(result)
    }

    fn check_dump(dump: &PartDump, work_path: &PathInfo) -> Result<RelFileInfo, MigError> {
        Ok(MigrateInfo::check_file(
            &dump.archive,
            &FileType::GZipTar,
            work_path,
        )?)
    }

    fn check_file(
        file_ref: &FileRef,
        expected_type: &FileType,
        work_path: &PathInfo,
    ) -> Result<RelFileInfo, MigError> {
        if let Some(file_info) = FileInfo::new(&file_ref, &work_path.path)? {
            // make sure files are present and in /workdir, generate total size and partitioning config in miginfo
            let rel_path = if let Some(ref rel_path) = file_info.rel_path {
                rel_path.clone()
            } else {
                error!("The file '{}' was found outside of the working directory. This setup is not supported", file_ref.path.display());
                return Err(MigError::displayed());
            };

            let os_api = OSApiImpl::new()?;
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
            Err(MigError::displayed())
        }
    }
}
