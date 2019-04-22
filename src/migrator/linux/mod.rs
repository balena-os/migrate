use failure::{Fail, ResultExt};
use libc::{getuid, sysinfo};
use log::{debug, error, info, trace, warn};
use serde_json::Value;
use std::fs::File;
use std::io::{BufReader, Read};
use std::time::Instant;

// use std::os::linux::{};

use regex::Regex;

mod path_info;
mod util;

use path_info::PathInfo;

use crate::migrator::{
    common::format_size_with_unit,
    linux::util::{
        call_cmd, expect_file, get_file_info, FileInfo, GRUB_INSTALL_CMD, LSBLK_CMD, MOKUTIL_CMD,
        UNAME_CMD,
    },
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
const MODULE: &str = "LinuxMigrator";
const OS_NAME_REGEX: &str = r#"^PRETTY_NAME="([^"]+)"$"#;

// DOS/MBR boot sector (gzip compressed data, was "resin-image-genericx86-64.resinos-img", last modified: Wed Mar 20 16:33:33 2019, from Unix)
const OS_IMG_FTYPE_REGEX: &str = r#"^DOS/MBR boot sector.*\(gzip compressed data.*\)$"#;
const OS_CFG_FTYPE_REGEX: &str = r#"^ASCII text$"#;

const OS_RELEASE_FILE: &str = "/etc/os-release";
const BOOT_DIR: &str = "/boot";
const ROOT_DIR: &str = "/";
const EFI_DIR: &str = "/boot/efi";

const UNAME_ARGS_OS_ARCH: [&str; 1] = ["-m"];

const GRUB_INST_VERSION_ARGS: [&str; 1] = ["--version"];
const GRUB_INST_VERSION_RE: &str = r#"^.*\s+\(GRUB\)\s+([0-9]+)\.([0-9]+)[^0-9].*$"#;
const GRUB_MIN_VERSION: &str = "2";

const LSBLK_REGEX: &str = r#"^(\d+)(\s+(.*))?$"#;

const MOKUTIL_ARGS_SB_STATE: [&str; 1] = ["--sb-state"];

const MIN_DISK_SIZE: u64 = 2 * 1024 * 1024 * 1024; // 2 GB

const OS_KERNEL_RELEASE_FILE: &str = "/proc/sys/kernel/osrelease";
const OS_MEMINFO_FILE: &str = "/proc/meminfo";

const SYS_UEFI_DIR: &str = "/sys/firmware/efi";

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
    /*
                os_name: None,
                os_release: None,
                os_arch: None,
                uefi_boot: None,
                boot_dev: None,
                mem_tot: None,
                mem_free: None,
                admin: None,
                sec_boot: None,
    */
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
        }
    }
}

pub struct LinuxMigrator {
    config: Config,
    sysinfo: SysInfo,
}

impl LinuxMigrator {
    pub fn migrate() -> Result<(), MigError> {
        let _migrator = LinuxMigrator::try_init(Config::new()?)?;
        Ok(())
    }

    pub fn try_init(config: Config) -> Result<LinuxMigrator, MigError> {
        trace!("LinuxMigrator::try_init: entered");

        info!("migrate mode: {:?}", config.migrate.mode);

        // create default
        let mut migrator = LinuxMigrator {
            config,
            sysinfo: SysInfo::default(),
        };

        // fake admin is not honored in release mode
        if !migrator.is_admin()? {
            error!("please run this program as root");
            return Err(MigError::from_remark(
                MigErrorKind::InvState,
                &format!("{}::try_init: was run without admin privileges", MODULE),
            ));
        }

        // is it even a supported OS ?
        migrator.sysinfo.os_name = Some(migrator.get_os_name()?);
        if let Some(ref os_name) = migrator.sysinfo.os_name {
            info!("OS Name is {}", os_name);
            if let None = SUPPORTED_OSSES.iter().position(|&r| r == os_name) {
                let message = format!("your OS '{}' is not in the list of operating systems supported by balena-migrate", os_name);
                error!("{}", &message);
                return Err(MigError::from_remark(MigErrorKind::InvState, &message));
            }
        }

        let mut work_dir = String::from("");

        // Check out relevant paths
        migrator.sysinfo.disk_info = Some(migrator.get_disk_info()?);
        if let Some(ref disk_info) = migrator.sysinfo.disk_info {
            info!(
                "Boot device is {}, size: {}",
                disk_info.disk_dev,
                format_size_with_unit(disk_info.disk_size)
            );
            if disk_info.disk_size < MIN_DISK_SIZE {
                let message = format!(
                    "The size of your harddrive {} = {} is too small to install balenaOS",
                    disk_info.disk_dev,
                    format_size_with_unit(disk_info.disk_size)
                );
                error!("{}", &message);
                return Err(MigError::from_remark(MigErrorKind::InvState, &message));
            }

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

            if let Some(ref file_info) = expect_file(
                &balena_cfg.config,
                "balena config",
                "got ASCII text",
                &work_dir,
                &Regex::new(OS_CFG_FTYPE_REGEX).unwrap(),
            )? {
                // TODO: check if valid, contents, report app
                let parse_res: Value = serde_json::from_reader(BufReader::new(
                    File::open(&file_info.path).context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "{}::try_init:cannot open file '{}'",
                            MODULE, &file_info.path
                        ),
                    ))?,
                ))
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("{}::new: failed to parse '{}'", MODULE, &file_info.path),
                ))?;

                if let Some(app) = parse_res.get("applicationName") {
                    info!("Configured for application: {}", app);
                } else {
                    let message = String::from("The balena config does not contain some required fields, please supply a valid config.json");
                    error!("{}", message);
                    return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
                }

                if let Some(dev_type) = parse_res.get("deviceType") {
                    info!("Configured for device type: {}", dev_type);
                } else {
                    let message = String::from("The balena config does not contain some required fields, please supply a valid config.json");
                    error!("{}", message);
                    return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
                }

                info!("The balena OS config looks ok: '{}'", file_info.path);
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

        migrator.sysinfo.os_arch = Some(migrator.get_os_arch()?);

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

        Ok(migrator)
    }

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
            self.init_rpi(
                captures.get(1).unwrap().as_str(),
                captures.get(2).unwrap().as_str(),
            )?;
        }

        if let Some(captures) = Regex::new(BB_MODEL_REGEX)
            .unwrap()
            .captures(&dev_tree_model)
        {
            self.init_bb(
                captures.get(1).unwrap().as_str(),
                captures.get(3).unwrap().as_str(),
            )?;
        }

        let message = format!(
            "Your device type: '{}' is not supported by balena-migrate.",
            dev_tree_model
        );
        error!("{}", message);
        Err(MigError::from_remark(MigErrorKind::InvState, &message))
    }

    fn init_rpi(&mut self, version: &str, model: &str) -> Result<(), MigError> {
        trace!(
            "LinuxMigrator::init_rpi: entered with type: '{}', model: '{}'",
            version,
            model
        );
        Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn init_bb(&mut self, cpu: &str, model: &str) -> Result<(), MigError> {
        trace!(
            "LinuxMigrator::init_bb: entered with type: '{}', model: '{}'",
            cpu,
            model
        );
        Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn init_amd64(&mut self) -> Result<(), MigError> {
        trace!("LinuxMigrator::init_amd64: entered");

        self.sysinfo.efi_boot = Some(self.is_uefi_boot()?);
        if let Some(efi_boot) = self.sysinfo.efi_boot {
            info!(
                "System is booted in {} mode",
                match efi_boot {
                    true => "EFI",
                    false => "Legacy BIOS",
                }
            );
            if efi_boot == true {
                // check for EFI dir & size

                self.sysinfo.secure_boot = Some(self.is_secure_boot()?);
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
        }

        let grub_version = self.get_grub_version()?;
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

    fn init_i386(&mut self) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }

    #[cfg(not(debug_assertions))]
    pub fn is_admin(&self) -> Result<bool, MigError> {
        trace!("LinuxMigrator::is_admin: entered");
        let admin = Some(unsafe { getuid() } == 0);
        Ok(admin.unwrap())
    }

    #[cfg(debug_assertions)]
    pub fn is_admin(&self) -> Result<bool, MigError> {
        trace!("LinuxMigrator::is_admin: entered");
        let admin = Some(unsafe { getuid() } == 0);
        Ok(admin.unwrap() | self.config.debug.fake_admin)
    }

    fn get_mem_info() -> Result<(u64, u64), MigError> {
        trace!("LinuxMigrator::get_mem_info: entered");
        // TODO: could add loads, uptime if needed
        use std::mem;
        let mut s_info: libc::sysinfo = unsafe { mem::uninitialized() };
        let res = unsafe { libc::sysinfo(&mut s_info) };
        if res == 0 {
            Ok((s_info.totalram as u64, s_info.freeram as u64))
        } else {
            Err(MigError::from(MigErrorKind::NotImpl))
        }
    }

    fn get_os_arch(&mut self) -> Result<OSArch, MigError> {
        trace!("LinuxMigrator::get_os_arch: entered");
        let cmd_res =
            call_cmd(UNAME_CMD, &UNAME_ARGS_OS_ARCH, true).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("{}::get_os_arch: call {}", MODULE, UNAME_CMD),
            ))?;

        if cmd_res.status.success() {
            if cmd_res.stdout.to_lowercase() == "x86_64" {
                Ok(OSArch::AMD64)
            } else if cmd_res.stdout.to_lowercase() == "i386" {
                Ok(OSArch::I386)
            } else if cmd_res.stdout.to_lowercase() == "armv7l" {
                Ok(OSArch::ARMHF)
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::get_os_arch: unsupported architectute '{}'",
                        MODULE, cmd_res.stdout
                    ),
                ))
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &format!(
                    "{}::get_os_arch: command failed: {}",
                    MODULE,
                    cmd_res.status.code().unwrap_or(0)
                ),
            ))
        }
    }

    fn get_grub_version(&mut self) -> Result<(String, String), MigError> {
        trace!("LinuxMigrator::get_grub_version: entered");
        let cmd_res = call_cmd(GRUB_INSTALL_CMD, &GRUB_INST_VERSION_ARGS, true).context(
            MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("{}::get_grub_version: call {}", MODULE, UNAME_CMD),
            ),
        )?;

        if cmd_res.status.success() {
            let re = Regex::new(GRUB_INST_VERSION_RE).unwrap();
            if let Some(captures) = re.captures(cmd_res.stdout.as_ref()) {
                Ok((
                    String::from(captures.get(1).unwrap().as_str()),
                    String::from(captures.get(2).unwrap().as_str()),
                ))
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::get_grub_version: failed to parse grub version string: {}",
                        MODULE, cmd_res.stdout
                    ),
                ))
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &format!(
                    "{}::get_os_arch: command failed: {}",
                    MODULE,
                    cmd_res.status.code().unwrap_or(0)
                ),
            ))
        }
    }

    fn is_uefi_boot(&mut self) -> Result<bool, MigError> {
        trace!("LinuxMigrator::is_uefi_boot: entered");
        match std::fs::metadata(SYS_UEFI_DIR) {
            Ok(metadata) => Ok(metadata.file_type().is_dir()),
            Err(why) => match why.kind() {
                std::io::ErrorKind::NotFound => Ok(false),
                _ => Err(MigError::from(why.context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("{}::is_uefi_boot: access {}", MODULE, SYS_UEFI_DIR),
                )))),
            },
        }
    }

    fn is_secure_boot(&mut self) -> Result<bool, MigError> {
        trace!("LinuxMigrator::is_secure_boot: entered");
        let cmd_res = match call_cmd(MOKUTIL_CMD, &MOKUTIL_ARGS_SB_STATE, true) {
            Ok(cr) => {
                debug!("{}::is_secure_boot: {} -> {:?}", MODULE, MOKUTIL_CMD, cr);
                cr
            }
            Err(why) => {
                debug!("{}::is_secure_boot: {} -> {:?}", MODULE, MOKUTIL_CMD, why);
                match why.kind() {
                    MigErrorKind::NotFound => {
                        return Ok(false);
                    }
                    _ => {
                        return Err(why);
                    }
                }
            }
        };

        let regex = Regex::new(r"^SecureBoot\s+(disabled|enabled)$").unwrap();
        let lines = cmd_res.stdout.lines();
        for line in lines {
            if let Some(cap) = regex.captures(line) {
                if cap.get(1).unwrap().as_str() == "enabled" {
                    return Ok(true);
                } else {
                    return Ok(false);
                }
            }
        }
        error!(
            "{}::is_secure_boot: failed to parse command output: '{}'",
            MODULE, cmd_res.stdout
        );
        Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!("{}::is_secure_boot: failed to parse command output", MODULE),
        ))
    }

    fn get_os_name(&self) -> Result<String, MigError> {
        trace!("LinuxMigrator::get_os_name: entered");
        if util::file_exists(OS_RELEASE_FILE) {
            // TODO: ensure availabilty of method / file exists
            if let Some(os_name) =
                util::parse_file(OS_RELEASE_FILE, &Regex::new(OS_NAME_REGEX).unwrap())?
            {
                Ok(os_name[1].clone())
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "{}::get_os_name: could not be located in file {}",
                        MODULE, OS_RELEASE_FILE
                    ),
                ))
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "{}::get_os_name: could not locate file {}",
                    MODULE, OS_RELEASE_FILE
                ),
            ))
        }
    }

    fn get_os_release(&mut self) -> Result<OSRelease, MigError> {
        let os_info =
            std::fs::read_to_string(OS_KERNEL_RELEASE_FILE).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("File read '{}'", OS_KERNEL_RELEASE_FILE),
            ))?;

        Ok(OSRelease::parse_from_str(&os_info.trim())?)
    }

    fn get_disk_info(&mut self) -> Result<DiskInfo, MigError> {
        trace!("LinuxMigrator::get_disk_info: entered");

        let mut disk_info = DiskInfo::default();

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

        disk_info.efi_path = PathInfo::new(EFI_DIR)?;
        if let Some(ref efi_part) = disk_info.efi_path {
            debug!("{}", efi_part);
        }

        disk_info.work_path = PathInfo::new(&self.config.migrate.work_dir)?;
        if let Some(ref work_part) = disk_info.work_path {
            debug!("{}", work_part);
        }

        disk_info.root_path = PathInfo::new(ROOT_DIR)?;

        if let Some(ref root_part) = disk_info.root_path {
            debug!("{}", root_part);

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
