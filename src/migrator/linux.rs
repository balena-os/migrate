use failure::ResultExt;
use log::{debug, error, info, trace, warn};

use regex::Regex;

mod path_info;
pub(crate) mod util;

use path_info::PathInfo;

use crate::migrator::{
    common::{
        balena_cfg_json::BalenaCfgJson, config::MigMode, file_info::FileInfo, format_size_with_unit,
    },
    linux::util::*,
    Config, MigErrCtx, MigError, MigErrorKind, OSArch, OSRelease,
};

const SUPPORTED_OSSES: &'static [&'static str] = &[
    "Ubuntu 18.04.2 LTS",
    "Ubuntu 16.04.2 LTS",
    "Ubuntu 14.04.2 LTS",
    "Raspbian GNU/Linux 9 (stretch)",
    "Debian GNU/Linux 9 (stretch)",
];

const DEVICE_TREE_MODEL: &str = "/proc/device-tree/model";
const RPI_MODEL_REGEX: &str = r#"^Raspberry\s+Pi\s+(\S+)\s+Model\s+(.*)$"#;
const BB_MODEL_REGEX: &str = r#"^((\S+\s+)*\S+)\s+BeagleBone\s+(\S+)$"#;
const MODULE: &str = "migrator::linux";

const OS_IMG_FTYPE_REGEX: &str = r#"^DOS/MBR boot sector.*\(gzip compressed data.*\)$"#;
const INITRD_FTYPE_REGEX: &str = r#"^ASCII cpio archive.*\(gzip compressed data.*\)$"#;

const OS_CFG_FTYPE_REGEX: &str = r#"^ASCII text.*$"#;

const BOOT_DIR: &str = "/boot";
const ROOT_DIR: &str = "/";
const EFI_DIR: &str = "/boot/efi";

const GRUB_MIN_VERSION: &str = "2";

const LSBLK_REGEX: &str = r#"^(\d+)(\s+(.*))?$"#;

const MIN_DISK_SIZE: u64 = 2 * 1024 * 1024 * 1024; // 2 GB

struct DiskInfo {
    disk_dev: String,
    disk_size: u64,
    disk_uuid: String,
    root_path: Option<PathInfo>,
    boot_path: Option<PathInfo>,
    efi_path: Option<PathInfo>,
    work_path: Option<PathInfo>,
}

impl DiskInfo {
    pub fn default() -> DiskInfo {
        DiskInfo {
            disk_dev: String::from(""),
            disk_uuid: String::from(""),
            disk_size: 0,
            root_path: None,
            boot_path: None,
            efi_path: None,
            work_path: None,
        }
    }
}

struct SysInfo {
    os_name: Option<String>,
    os_release: Option<OSRelease>,
    os_arch: Option<OSArch>,
    efi_boot: Option<bool>,
    secure_boot: Option<bool>,
    disk_info: Option<DiskInfo>,
    image_info: Option<FileInfo>,
    kernel_info: Option<FileInfo>,
    initrd_info: Option<FileInfo>,
    device_slug: Option<String>,
}

impl SysInfo {
    pub fn default() -> SysInfo {
        SysInfo {
            os_name: None,
            os_release: None,
            os_arch: None,
            efi_boot: None,
            secure_boot: None,
            disk_info: None,
            image_info: None,
            kernel_info: None,
            initrd_info: None,
            device_slug: None,
        }
    }

    pub fn is_efi_boot(&self) -> bool {
        if let Some(efi_boot) = self.efi_boot {
            efi_boot
        } else {
            false
        }
    }
}

pub struct LinuxMigrator {
    config: Config,
    sysinfo: SysInfo,
}

impl LinuxMigrator {
    pub fn migrate() -> Result<(), MigError> {
        let migrator = LinuxMigrator::try_init(Config::new()?)?;
        match migrator.config.migrate.mode {
            MigMode::IMMEDIATE => migrator.do_migrate(),
            MigMode::PRETEND => Ok(()),
            MigMode::AGENT => Err(MigError::from(MigErrorKind::NotImpl)),
        }
    }

    // **********************************************************************
    // ** Initialise migrator
    // **********************************************************************

    pub fn try_init(config: Config) -> Result<LinuxMigrator, MigError> {
        trace!("LinuxMigrator::try_init: entered");

        info!("migrate mode: {:?}", config.migrate.mode);

        // create default
        let mut migrator = LinuxMigrator {
            config,
            sysinfo: SysInfo::default(),
        };

        // **********************************************************************
        // We need to be root to do this
        // note: fake admin is not honored in release mode

        if !is_admin(migrator.config.debug.fake_admin)? {
            error!("please run this program as root");
            return Err(MigError::from_remark(
                MigErrorKind::InvState,
                &format!("{}::try_init: was run without admin privileges", MODULE),
            ));
        }

        // **********************************************************************
        // Check if we are on a supported OS.
        // Add OS string to SUPPORTED_OSSES list above  once tested

        migrator.sysinfo.os_name = Some(get_os_name()?);
        if let Some(ref os_name) = migrator.sysinfo.os_name {
            info!("OS Name is {}", os_name);
            if let None = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
                let message = format!("your OS '{}' is not in the list of operating systems supported by balena-migrate", os_name);
                error!("{}", &message);
                return Err(MigError::from_remark(MigErrorKind::InvState, &message));
            }
        }

        // **********************************************************************
        // Run the architecture dependent part of initialization
        // Add further architectures / functons here

        migrator.sysinfo.os_arch = Some(get_os_arch()?);
        if let Some(ref os_arch) = migrator.sysinfo.os_arch {
            info!("OS Architecture is {}", os_arch);
            match os_arch {
                OSArch::ARMHF => {
                    migrator.init_armhf()?;
                }
                OSArch::AMD64 => {
                    migrator.init_amd64()?;
                }
                OSArch::I386 => {
                    migrator.init_i386()?;
                }
                _ => {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!(
                            "{}::try_init: unexpected OsArch encountered: {}",
                            MODULE, os_arch
                        ),
                    ));
                }
            }
        }

        // **********************************************************************
        // Set the custom device slug here if configured

        if let Some(ref force_slug) = migrator.config.migrate.force_slug {
            warn!(
                "setting device type to '{}' using 'force_slug, detected type was '{}'",
                force_slug,
                migrator.sysinfo.device_slug.unwrap()
            );
            migrator.sysinfo.device_slug = Some(force_slug.clone());
        }

        // **********************************************************************
        // Check the disk for required paths / structure / size

        let mut work_dir = String::from("");

        // Check out relevant paths
        migrator.sysinfo.disk_info = Some(migrator.get_disk_info()?);
        if let Some(ref disk_info) = migrator.sysinfo.disk_info {
            info!(
                "Boot device is {}, size: {}",
                disk_info.disk_dev,
                format_size_with_unit(disk_info.disk_size)
            );

            // **********************************************************************
            // Require a minimum disk device size for installation

            if disk_info.disk_size < MIN_DISK_SIZE {
                let message = format!(
                    "The size of your harddrive {} = {} is too small to install balenaOS",
                    disk_info.disk_dev,
                    format_size_with_unit(disk_info.disk_size)
                );
                error!("{}", &message);
                return Err(MigError::from_remark(MigErrorKind::InvState, &message));
            }

            // **********************************************************************
            // Check if work_dir was found

            if let Some(ref work_dir_info) = disk_info.work_path {
                work_dir = work_dir_info.path.clone();
            // TODO: check available space ..
            } else {
                let message = format!(
                    "the working directory '{}' could not be accessed",
                    migrator.config.migrate.work_dir
                );
                error!("{}", &message);
                return Err(MigError::from_remark(MigErrorKind::InvState, &message));
            }
        }

        // **********************************************************************
        // Check migrate config section

        let kernel_regex = if let Some(ref os_arch) = migrator.sysinfo.os_arch {
            match os_arch {
                    OSArch::AMD64 => r#"^Linux kernel x86 boot executable bzImage, version \d+\.\d+\.\d+-yocto-standard.*$"#,
                    OSArch::ARMHF => r#"^Linux kernel ARM boot executable zImage \(little-endian\).*$"#,
                    _ => { panic!("unexpected OS Architecture: {:?}", os_arch); },
                }
        } else {
            panic!("unset OS Architecture");
        };

        // TODO: this extra space would be somehow dependent on FS block size & other overheads
        let mut boot_required_space: u64 = 8192;

        if let Some(file_info) = expect_file(
            &migrator.config.migrate.kernel_file,
            "migrate kernel",
            "Linux Kernel",
            &work_dir,
            &Regex::new(kernel_regex).unwrap(),
        )? {
            info!("The balena migrate kernel looks ok: '{}'", &file_info.path);
            boot_required_space += file_info.size;
            migrator.sysinfo.kernel_info = Some(file_info);
        } else {
            let message = String::from("The migrate kernel has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }

        if let Some(file_info) = expect_file(
            &migrator.config.migrate.initramfs_file,
            "migrate initramfs",
            "ASCII cpio archive",
            &work_dir,
            &Regex::new(INITRD_FTYPE_REGEX).unwrap(),
        )? {
            info!(
                "The balena migrate initramfs looks ok: '{}'",
                &file_info.path
            );
            boot_required_space += file_info.size;
            migrator.sysinfo.initrd_info = Some(file_info);
        } else {
            let message = String::from("The migrate initramfs has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }

        // **********************************************************************
        // Check available space on /boot / /boot/efi

        let kernel_path_info = if let Some(ref disk_info) = migrator.sysinfo.disk_info {
            if migrator.sysinfo.is_efi_boot() == true {
                if let Some(ref efi_path) = disk_info.efi_path {
                    // TODO: add required space for efi boot files
                    efi_path
                } else {
                    panic!("no {} path info found", EFI_DIR)
                }
            } else {
                if let Some(ref boot_path) = disk_info.boot_path {
                    boot_path
                } else {
                    panic!("no {} path info found", BOOT_DIR)
                }
            }
        } else {
            panic!("no disk info found")
        };

        if kernel_path_info.fs_free < boot_required_space {
            let message = format!("We have not found sufficient space for the migrate boot environment in {}. {} of free space are required.", kernel_path_info.path, format_size_with_unit(boot_required_space));
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }

        // **********************************************************************
        // Check balena config section

        if let Some(ref balena_cfg) = migrator.config.balena {
            // check balena os image
            if let Some(file_info) = expect_file(
                &balena_cfg.image,
                "balena image",
                "DOS/MBR boot sector in gzip compressed data",
                &work_dir,
                &Regex::new(OS_IMG_FTYPE_REGEX).unwrap(),
            )? {
                info!("The balena OS image looks ok: '{}'", file_info.path);
                migrator.sysinfo.image_info = Some(file_info);
            } else {
                let message = String::from("The balena image has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
                error!("{}", message);
                return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
            }

            // check balena os config
            if let Some(ref file_info) = expect_file(
                &balena_cfg.config,
                "balena config",
                "ASCII text",
                &work_dir,
                &Regex::new(OS_CFG_FTYPE_REGEX).unwrap(),
            )? {
                if let Some(ref device_slug) = migrator.sysinfo.device_slug {
                    let balena_cfg_json = BalenaCfgJson::new(&file_info.path)?;
                    balena_cfg_json.check(device_slug)?;
                } else {
                    panic!("no device slug given in sysinfo");
                }
            // TODO: check if valid, contents, report app
            } else {
                let message = String::from("The balena config has not been specified or cannot be accessed. Automatic download is not yet implemented, so you need to specify and supply all required files");
                error!("{}", message);
                return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
            }
        } else {
            let message = String::from("The balena section of the configuration is empty. Automatic download is not yet implemented, so you need to specify and supply all required files and options.");
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
        }

        Ok(migrator)
    }

    // **********************************************************************
    // ** Start the actual migration
    // **********************************************************************

    fn do_migrate(&self) -> Result<(), MigError> {
        if let Some(ref device_slug) = self.sysinfo.device_slug {
            match device_slug.as_ref() {
                "beaglebone-green" => {
                    self.setup_bbg()?;
                }
                _ => {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!(
                            "{}::try_init: unexpected device type encountered: {}",
                            MODULE, &device_slug
                        ),
                    ));
                }
            }
        }

        Ok(())
    }

    // **********************************************************************
    // ** ARMHF specific initialisation
    // **********************************************************************

    fn init_armhf(&mut self) -> Result<(), MigError> {
        trace!("LinuxMigrator::init_armhf: entered");
        // Raspberry Pi 3 Model B Rev 1.2

        let dev_tree_model =
            std::fs::read_to_string(DEVICE_TREE_MODEL).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "{}::init_armhf: unable to determine model due to inaccessible file '{}'",
                    MODULE, DEVICE_TREE_MODEL
                ),
            ))?;

        if let Some(captures) = Regex::new(RPI_MODEL_REGEX)
            .unwrap()
            .captures(&dev_tree_model)
        {
            return Ok(self.init_rpi(
                captures.get(1).unwrap().as_str(),
                captures
                    .get(2)
                    .unwrap()
                    .as_str()
                    .trim_matches(char::from(0)),
            )?);
        }

        if let Some(captures) = Regex::new(BB_MODEL_REGEX)
            .unwrap()
            .captures(&dev_tree_model)
        {
            return Ok(self.init_bb(
                captures.get(1).unwrap().as_str(),
                captures
                    .get(3)
                    .unwrap()
                    .as_str()
                    .trim_matches(char::from(0)),
            )?);
        }

        let message = format!(
            "Your device type: '{}' is not supported by balena-migrate.",
            dev_tree_model
        );
        error!("{}", message);
        Err(MigError::from_remark(MigErrorKind::InvState, &message))
    }

    // **********************************************************************
    // ** RPI specific initialisation
    // **********************************************************************

    fn init_rpi(&mut self, version: &str, model: &str) -> Result<(), MigError> {
        trace!(
            "LinuxMigrator::init_rpi: entered with type: '{}', model: '{}'",
            version,
            model
        );
        // TODO: set / check device type

        Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn init_bb(&mut self, cpu: &str, model: &str) -> Result<(), MigError> {
        trace!(
            "LinuxMigrator::init_bb: entered with type: '{}', model: '{}'",
            cpu,
            model
        );

        self.sysinfo.device_slug = match model {
            "Green" => Some(String::from("beaglebone-green")),
            _ => {
                let message = format!("The beaglebone model reported by your device ('{}') is not supported by balena-migrate", model);
                error!("{}", message);
                return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
            }
        };

        Ok(())
    }

    fn setup_bbg(&self) -> Result<(), MigError> {
        trace!(
            "LinuxMigrator::setup_bb: entered with type: '{}'",
            match &self.sysinfo.device_slug {
                Some(s) => s,
                _ => panic!("no device type slug found"),
            }
        );

        Ok(())
    }

    // **********************************************************************
    // ** AMD64 specific initialisation
    // **********************************************************************

    fn init_amd64(&mut self) -> Result<(), MigError> {
        trace!("LinuxMigrator::init_amd64: entered");

        self.sysinfo.device_slug = Some(String::from("intel-nuc"));
        self.sysinfo.efi_boot = Some(is_uefi_boot()?);

        info!(
            "System is booted in {} mode",
            match self.sysinfo.is_efi_boot() {
                true => "EFI",
                false => "Legacy BIOS",
            }
        );

        if self.sysinfo.is_efi_boot() == true {
            // check for EFI dir & size
            self.sysinfo.secure_boot = Some(is_secure_boot()?);
            if let Some(secure_boot) = self.sysinfo.secure_boot {
                info!(
                    "Secure boot is {}enabled",
                    match secure_boot {
                        true => "",
                        false => "not ",
                    }
                );
                if secure_boot == true {
                    let message = format!("balena-migrate does not currently support systems with secure boot enabled.");
                    error!("{}", &message);
                    return Err(MigError::from_remark(MigErrorKind::InvState, &message));
                }
            }
        } else {
            self.sysinfo.secure_boot = Some(false);
            info!("Assuming that Secure boot is not enabled for Legacy BIOS system");
        }

        let grub_version = get_grub_version()?;
        info!(
            "grub-install version is {}.{}",
            grub_version.0, grub_version.1
        );
        if grub_version.0 < String::from(GRUB_MIN_VERSION) {
            let message = format!("your version of grub-install ({}.{}) is not supported. balena-migrate requires grub version 2 or higher.", grub_version.0, grub_version.1);
            error!("{}", &message);
            return Err(MigError::from_remark(MigErrorKind::InvState, &message));
        }

        Ok(())
    }

    // **********************************************************************
    // ** I386 specific initialisation
    // **********************************************************************

    fn init_i386(&mut self) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }

    // **********************************************************************
    // ** Check required paths on disk
    // **********************************************************************

    fn get_disk_info(&mut self) -> Result<DiskInfo, MigError> {
        trace!("LinuxMigrator::get_disk_info: entered");

        let mut disk_info = DiskInfo::default();

        // **********************************************************************
        // check /boot

        disk_info.boot_path = PathInfo::new(BOOT_DIR)?;
        if let Some(ref boot_part) = disk_info.boot_path {
            debug!("{}", boot_part);
        } else {
            let message = format!(
                "Unable to retrieve attributes for {} file system, giving up.",
                BOOT_DIR
            );
            error!("{}", message);
            return Err(MigError::from_remark(MigErrorKind::InvState, &message));
        }

        if self.sysinfo.is_efi_boot() == true {
            // **********************************************************************
            // check /boot/efi
            // TODO: detect efi dir in other locations (via parted / mount)
            disk_info.efi_path = PathInfo::new(EFI_DIR)?;
            if let Some(ref efi_part) = disk_info.efi_path {
                debug!("{}", efi_part);
            }
        }

        // **********************************************************************
        // check work_dir

        disk_info.work_path = PathInfo::new(&self.config.migrate.work_dir)?;
        if let Some(ref work_part) = disk_info.work_path {
            debug!("{}", work_part);
        }

        // **********************************************************************
        // check /

        disk_info.root_path = PathInfo::new(ROOT_DIR)?;

        if let Some(ref root_part) = disk_info.root_path {
            debug!("{}", root_part);

            // **********************************************************************
            // Make sure all relevant paths are on one drive

            if let Some(ref boot_part) = disk_info.boot_path {
                if root_part.drive != boot_part.drive {
                    let message = "Your device has a disk layout that is incompatible with balena-migrate. balena migrate requires the /boot /boot/efi and / partitions to be on one drive";
                    error!("{}", message);
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!("{}::get_disk_info: {}", MODULE, message),
                    ));
                }
            }

            if let Some(ref efi_part) = disk_info.efi_path {
                if root_part.drive != efi_part.drive {
                    let message = "Your device has a disk layout that is incompatible with balena-migrate. balena migrate requires the /boot /boot/efi and / partitions to be on one drive";
                    error!("{}", message);
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!("{}::get_disk_info: {}", MODULE, message),
                    ));
                }
            }

            // **********************************************************************
            // get size & UUID of installation drive

            let args: Vec<&str> = vec!["-b", "--output=SIZE,UUID", &root_part.drive];

            let cmd_res = call_cmd(LSBLK_CMD, &args, true)?;
            if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
                return Err(MigError::from_remark(
                    MigErrorKind::ExecProcess,
                    &format!(
                        "{}::new: failed to retrieve device attributes for {}",
                        MODULE, &root_part.drive
                    ),
                ));
            }

            // debug!("lsblk output: {:?}",&cmd_res.stdout);
            let output: Vec<&str> = cmd_res.stdout.lines().collect();
            if output.len() < 2 {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::new: failed to parse block device attributes for {}",
                        MODULE, &root_part.drive
                    ),
                ));
            }

            debug!("lsblk output: {:?}", &output[1]);
            if let Some(captures) = Regex::new(LSBLK_REGEX).unwrap().captures(&output[1]) {
                disk_info.disk_size = captures.get(1).unwrap().as_str().parse::<u64>().unwrap();
                if let Some(cap) = captures.get(3) {
                    disk_info.disk_uuid = String::from(cap.as_str());
                }
            }
            disk_info.disk_dev = root_part.drive.clone();

            Ok(disk_info)
        } else {
            let message = format!(
                "Unable to retrieve attributes for {} file system, giving up.",
                ROOT_DIR
            );
            error!("{}", message);
            Err(MigError::from_remark(MigErrorKind::InvState, &message))
        }
    }

    /*
    fn get_mem_info1(&mut self) -> Result<(),MigError> {
         debug!("{}::get_mem_info: entered", MODULE);
         let mem_info = std::fs::read_to_string(OS_MEMINFO_FILE).context(MigErrCtx::from(MigErrorKind::Upstream))?;
         let lines = mem_info.lines();

         let regex_tot = Regex::new(r"^MemTotal:\s+(\d+)\s+(\S+)$").unwrap();
         let regex_free = Regex::new(r"^MemFree:\s+(\d+)\s+(\S+)$").unwrap();
         let mut found = 0;
         for line in lines {
             if let Some(cap) = regex_tot.captures(line) {
                let unit = cap.get(2).unwrap().as_str();
                if unit == "kB" {
                    self.mem_tot = Some(cap.get(1).unwrap().as_str().parse::<usize>().unwrap() * 1024);
                    found += 1;
                    if found > 1 {
                        break;
                    } else {
                        continue;
                    }
                } else {
                    // TODO: support other units
                    return Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::get_mem_info: unsupported unit {}", MODULE, unit)));
                }
             }

             if let Some(cap) = regex_free.captures(line) {
                let unit = cap.get(2).unwrap().as_str();
                if unit == "kB" {
                    self.mem_free = Some(cap.get(1).unwrap().as_str().parse::<usize>().unwrap() * 1024);
                    found += 1;
                    if found > 1 {
                        break;
                    } else {
                        continue;
                    }
                } else {
                    // TODO: support other units
                    return Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::get_mem_info: unsupported unit {}", MODULE, unit)));
                }
             }
         }

         if let Some(_v) = self.mem_tot {
             if let Some(_v) = self.mem_free {
                return Ok(());
             }
         }

        Err(MigError::from_remark(MigErrorKind::NotFound, &format!("{}::get_mem_info: failed to retrieve required memory values", MODULE)))
    }
    */
}

/*
impl Migrator for LinuxMigrator {





    fn get_mem_tot(&mut self) -> Result<u64, MigError> {
        match self.mem_tot {
            Some(m) => Ok(m),
            None => {
                self.get_mem_info()?;
                Ok(self.mem_tot.unwrap())
            }
        }
    }

    fn get_mem_avail(&mut self) -> Result<u64, MigError> {
        match self.mem_free {
            Some(m) => Ok(m),
            None => {
                self.get_mem_info()?;
                Ok(self.mem_free.unwrap())
            }
        }
    }


    fn can_migrate(&mut self) -> Result<bool, MigError> {
        debug!("{}::can_migrate: entered", MODULE);
        if ! self.is_admin()? {
            warn!("{}::can_migrate: you need to run this program as root", MODULE);
            return Ok(false);
        }

        if self.is_secure_boot()? {
            warn!("{}::can_migrate: secure boot appears to be enabled. Please disable secure boot in the firmware settings.", MODULE);
            return Ok(false);
        }

        if let Some(ref balena) = self.config.balena {
            if balena.api_check == true {
                info!("{}::can_migrate: checking connection api backend at to {}:{}", MODULE, balena.api_host, balena.api_port );
                let now = Instant::now();
                if let Err(why) = check_tcp_connect(&balena.api_host, balena.api_port, balena.check_timeout) {
                    warn!("{}::can_migrate: connectivity check to {}:{} failed timeout {} seconds ", MODULE, balena.api_host, balena.api_port, balena.check_timeout );
                    warn!("{}::can_migrate: check_tcp_connect returned: {:?} ", MODULE, why );
                    return Ok(false);
                }
                info!("{}::can_migrate: successfully connected to api backend in {} ms", MODULE, now.elapsed().as_millis());
            }

            if balena.vpn_check == true {
                info!("{}::can_migrate: checking connection vpn backend at to {}:{}", MODULE, balena.vpn_host, balena.vpn_port);
                let now = Instant::now();
                if let Err(why) = check_tcp_connect(&balena.vpn_host, balena.vpn_port, balena.check_timeout) {
                    warn!("{}::can_migrate: connectivity check to {}:{} failed timeout {} seconds ", MODULE, balena.vpn_host, balena.vpn_port, balena.check_timeout );
                    warn!("{}::can_migrate: check_tcp_connect returned: {:?} ", MODULE, why );
                    return Ok(false);
                }
                info!("{}::can_migrate: successfully connected to vpn backend in {} ms", MODULE, now.elapsed().as_millis());
            }
        }

        if self.config.migrate.kernel_file.is_empty() {
            warn!("{}::can_migrate: no migration kernel file was confgured. Plaese adapt your configuration to supply a valid kernel file .", MODULE);
        }

        if self.config.migrate.initramfs_file.is_empty() {
            warn!("{}::can_migrate: no migration initramfs file was confgured. Plaese adapt your configuration to supply a valid initramfs file .", MODULE);
        }


        Ok(true)
    }

    fn migrate(&mut self) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
}
*/
