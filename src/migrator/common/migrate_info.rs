use lazy_static::lazy_static;
use log::{debug, error, info, warn};
use std::path::Path;

use crate::{
    common::{
        config::{ImageSource, ImageType, MigrateWifis, PartDump},
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
use crate::common::device_info::DeviceInfo;
use crate::common::file_digest::{check_digest, HashInfo};
use crate::common::{path_append, MigErrCtx};
pub(crate) use balena_cfg_json::BalenaCfgJson;
use failure::ResultExt;
use regex::Regex;
use std::fs::read_to_string;

#[derive(Debug)]
pub(crate) struct MigrateInfo {
    pub os_name: String,
    pub os_arch: OSArch,

    pub work_path: PathInfo,
    pub log_path: Option<DeviceInfo>,

    pub nwmgr_files: Vec<FileInfo>,
    pub wifis: Vec<WifiConfig>,

    image_file: Option<CheckedImageType>,
    pub config_file: BalenaCfgJson,
    // pub digests: HashMap<PathBuf, HashInfo>,
}

// TODO: sort out error reporting with Displayed

impl MigrateInfo {
    fn check_md5(base_path: &Path, md5_digests: &Path) -> Result<(), MigError> {
        lazy_static! {
            static ref LINE_SPIT_RE: Regex = Regex::new(r##"^(\S+)\s+(.*)$"##).unwrap();
        }

        let md5_path = path_append(base_path, md5_digests);

        for md5_line in read_to_string(&md5_path)
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to read file '{}'", md5_path.display()),
            ))?
            .lines()
        {
            if let Some(captures) = LINE_SPIT_RE.captures(md5_line) {
                let md5_sum = String::from(captures.get(1).unwrap().as_str());
                let path = path_append(base_path, captures.get(2).unwrap().as_str());
                let hash_info = HashInfo::Md5(md5_sum);
                if !check_digest(path.as_path(), &hash_info)? {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!("Failed to check digest on file: '{}'", path.display()),
                    ));
                }
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!("Encountered invalid line in md5 sums: '{}'", md5_line),
                ));
            }
        }

        Ok(())
    }

    #[allow(clippy::cognitive_complexity)] //TODO refactor this function to fix the clippy warning
    pub(crate) fn new(config: &Config) -> Result<MigrateInfo, MigError> {
        debug!("new: entered");
        let os_api = OSApiImpl::new()?;
        let os_arch = os_api.get_os_arch()?;
        let work_path = os_api.path_info_from_path(config.get_work_dir())?;
        let work_dir = &work_path.path;

        info!(
            "Working directory is '{}' on drive '{}', partition: '{}'",
            work_dir.display(),
            work_path.device_info.drive,
            work_path.device_info.device
        );

        if let Some(md5_sums) = config.get_md5_sums() {
            MigrateInfo::check_md5(&work_path.path, &md5_sums)?
        }

        let log_info = if let Some(log_dev) = config.get_log_device() {
            debug!("Checking log device: '{:?}'", log_dev);
            match os_api.device_info_from_devspec(log_dev) {
                Ok(dev_info) => {
                    info!("Using log path: '{}'", dev_info.get_alt_path().display());
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

        debug!("Checking config.json: '{:?}'", config.get_config_path());
        let config_file = if let Some(file_info) =
            FileInfo::new(config.get_config_path(), &work_dir)?
        {
            if file_info.rel_path.is_none() {
                error!("The balena OS config was found outside of the working directory. This setup is not supported");
                return Err(MigError::displayed());
            }

            let cfg_path_info = os_api.path_info_from_path(&file_info.path)?;
            if cfg_path_info.mountpoint != work_path.mountpoint {
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

        let mut nwmgr_files: Vec<FileInfo> = Vec::new();

        for file in config.get_nwmgr_files() {
            if let Some(file_info) = FileInfo::new(file.clone(), &work_dir)? {
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

        let wifi_cfg = config.get_wifis();
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

        if nwmgr_files.is_empty() && wifis.is_empty() && config.require_nwmgr_configs() {
            error!(
                "No Network manager files were found, the device might not be able to come online"
            );
            return Err(MigError::from(MigErrorKind::Displayed));
        }

        let result = MigrateInfo {
            os_name: os_api.get_os_name()?,
            os_arch,
            work_path,
            log_path: log_info,
            image_file: None,
            nwmgr_files,
            config_file,
            wifis,
        };

        debug!("MigrateInfo: {:?}", result);

        Ok(result)
    }

    pub fn set_os_image(&mut self, img_path: &ImageType) -> Result<(), MigError> {
        debug!("Checking image files: {:?}", img_path);

        self.image_file = Some(match img_path {
            ImageType::Flasher(ref flasher_img) => {
                if let ImageSource::File(ref flasher_img) = flasher_img {
                    let checked_ref = MigrateInfo::check_file(
                        &flasher_img,
                        &FileType::GZipOSImage,
                        &self.work_path,
                    )?;

                    CheckedImageType::Flasher(checked_ref)
                } else {
                    error!("Invalid image type '{:?}' for set_os_image", img_path);
                    return Err(MigError::displayed());
                }
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
                        archive: MigrateInfo::check_dump(&fs_dump.boot, &self.work_path)?,
                        blocks: fs_dump.boot.blocks,
                    },
                    root_a: CheckedPartDump {
                        archive: MigrateInfo::check_dump(&fs_dump.root_a, &self.work_path)?,
                        blocks: fs_dump.root_a.blocks,
                    },
                    root_b: CheckedPartDump {
                        archive: MigrateInfo::check_dump(&fs_dump.root_b, &self.work_path)?,
                        blocks: fs_dump.root_b.blocks,
                    },
                    state: CheckedPartDump {
                        archive: MigrateInfo::check_dump(&fs_dump.state, &self.work_path)?,
                        blocks: fs_dump.state.blocks,
                    },
                    data: CheckedPartDump {
                        archive: MigrateInfo::check_dump(&fs_dump.data, &self.work_path)?,
                        blocks: fs_dump.data.blocks,
                    },
                })
            }
        });
        Ok(())
    }

    pub fn get_os_image(&self) -> CheckedImageType {
        if let Some(ref os_image) = self.image_file {
            os_image.clone()
        } else {
            panic!("OS Image was not set");
        }
    }

    fn check_dump(dump: &PartDump, work_path: &PathInfo) -> Result<RelFileInfo, MigError> {
        Ok(MigrateInfo::check_file(
            &dump.archive,
            &FileType::GZipTar,
            work_path,
        )?)
    }

    fn check_file(
        file: &Path,
        expected_type: &FileType,
        work_path: &PathInfo,
    ) -> Result<RelFileInfo, MigError> {
        if let Some(file_info) = FileInfo::new(&file, &work_path.path)? {
            // make sure files are present and in /workdir, generate total size and partitioning config in miginfo
            let rel_path = if let Some(ref rel_path) = file_info.rel_path {
                rel_path.clone()
            } else {
                error!("The file '{}' was found outside of the working directory. This setup is not supported", file.display());
                return Err(MigError::displayed());
            };

            let os_api = OSApiImpl::new()?;
            let file_path_info = os_api.path_info_from_path(&file_info.path)?;
            if file_path_info.mountpoint != work_path.mountpoint {
                error!("The file '{}' appears to reside on a different partition from the working directory. This setup is not supported", file.display());
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
                        file.display(),
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
            error!("The balena file: '{}' can not be accessed.", file.display());
            Err(MigError::displayed())
        }
    }

    pub fn get_api_key(&self) -> Option<String> {
        self.config_file.get_api_key()
    }

    pub fn get_api_endpoint(&self) -> String {
        self.config_file.get_api_endpoint()
    }
}
