use log::{debug, error, info, trace, warn};
use std::path::PathBuf;

use crate::{
    common::{
        balena_cfg_json::BalenaCfgJson, format_size_with_unit, Config, FileInfo, FileType,
        MigError, MigErrorKind, MigrateWifis, OSArch,
    },
    defs::{BOOT_PATH, MEM_THRESHOLD, ROOT_PATH},
    linux_common::{
        ensured_commands::EnsuredCommands, get_mem_info, get_os_arch, get_os_name, WifiConfig,
        CHMOD_CMD, DF_CMD, FDISK_CMD, FILE_CMD, LSBLK_CMD, MKTEMP_CMD, MOUNT_CMD, REBOOT_CMD,
        UNAME_CMD,
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
pub(crate) use path_info::PathInfo;

#[derive(Debug)]
pub(crate) struct MigrateInfo {
    pub os_name: String,
    pub os_arch: OSArch,

    pub lsblk_info: LsblkInfo,
    pub root_path: PathInfo,
    pub boot_path: PathInfo,
    pub work_path: PathInfo,
    pub log_path: Option<(PathBuf, String)>,

    pub nwmgr_files: Vec<FileInfo>,
    pub wifis: Vec<WifiConfig>,

    pub image_file: FileInfo,
    pub config_file: BalenaCfgJson,
    pub kernel_file: FileInfo,
    pub initrd_file: FileInfo,
    pub dtb_file: Option<FileInfo>,

    pub cmds: EnsuredCommands,
}

const REQUIRED_CMDS: &'static [&'static str] = &[
    DF_CMD, LSBLK_CMD, FILE_CMD, UNAME_CMD, MOUNT_CMD, REBOOT_CMD, CHMOD_CMD, FDISK_CMD, MKTEMP_CMD,
];

// TODO: /etc path just in case

impl MigrateInfo {
    pub(crate) fn new(config: &Config) -> Result<MigrateInfo, MigError> {
        trace!("new: entered");
        // TODO: check files configured in config & create file_infos

        let mut cmds = EnsuredCommands::new(REQUIRED_CMDS)?;

        let os_arch = get_os_arch(&cmds)?;

        let lsblk_info = LsblkInfo::new(&cmds)?;

        // get all required drives
        let root_path = if let Some(path_info) = PathInfo::new(&cmds, ROOT_PATH, &lsblk_info)? {
            path_info
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "the device for path '{}' could not be established",
                    ROOT_PATH
                ),
            ));
        };
        let boot_path = if let Some(path_info) = PathInfo::new(&cmds, BOOT_PATH, &lsblk_info)? {
            path_info
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "the device for path '{}' could not be established",
                    BOOT_PATH
                ),
            ));
        };

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
            let (_lsblk_drive, lsblk_part) = lsblk_info.get_devinfo_from_partition(log_dev)?;
            Some((
                lsblk_part.get_path(),
                if let Some(ref fs_type) = lsblk_part.fstype {
                    fs_type.clone()
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvState,
                        &format!("Log fstype was not initialized for '{}'", log_dev.display()),
                    ));
                },
            ))
        } else {
            None
        };

        let work_dir = &work_path.path;
        info!("Working directory is '{}'", work_dir.display());

        let image_file = if let Some(file_info) =
            FileInfo::new(&config.balena.get_image_path(), &work_dir)?
        {
            file_info.expect_type(&cmds, &FileType::OSImage)?;
            info!(
                "The balena OS image looks ok: '{}'",
                file_info.path.display()
            );
            // TODO: make sure there is enough memory for OSImage

            // TODO: do this in linux_migrator.rs ?
            let required_mem = file_info.size + MEM_THRESHOLD;
            if get_mem_info()?.0 < required_mem {
                let message = format!("We have not found sufficient memory to store the balena OS image in ram. at least {} of memory is required.", format_size_with_unit(required_mem));
                error!("{}", message);
                return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
            }

            file_info
        } else {
            let message = String::from("The balena image has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        };

        let config_file = if let Some(file_info) =
            FileInfo::new(&config.balena.get_config_path(), &work_dir)?
        {
            file_info.expect_type(&cmds, &FileType::Json)?;

            let required_mem = file_info.size + MEM_THRESHOLD;
            if get_mem_info()?.0 < required_mem {
                let message = format!("We have not found sufficient memory to store the balena OS image in ram. at least {} of memory is required.", format_size_with_unit(required_mem));
                error!("{}", message);
                return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
            }

            let balena_cfg = BalenaCfgJson::new(file_info)?;
            info!(
                "The balena config file looks ok: '{}'",
                balena_cfg.get_path().display()
            );
            // TODO: make sure there is enough memory for OSImage

            // TODO: do this in linux_migrator.rs ?

            balena_cfg
        } else {
            let message = String::from("The balena image has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
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
            let message = String::from("The migrate kernel has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
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
            let message = String::from("The migrate initramfs has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
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
                let message = String::from("The migrate device tree blob has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
                error!("{}", message);
                return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
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
                let message = format!(
                    "The network manager config file '{}' could not be found",
                    file.display()
                );
                error!("{}", message);
                return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
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

        let result = MigrateInfo {
            os_name: get_os_name()?,
            os_arch,
            lsblk_info,
            root_path,
            boot_path,
            work_path,
            log_path,
            image_file,
            kernel_file,
            initrd_file,
            dtb_file,
            nwmgr_files,
            config_file,
            wifis,
            cmds,
        };

        debug!("Diskinfo: {:?}", result);

        Ok(result)
    }
}
