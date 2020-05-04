#[warn(clippy::missing_safety_doc)]
use failure::ResultExt;

use lazy_static::lazy_static;
use log::{debug, error, info, trace, warn};
use regex::{Regex, RegexBuilder};
use std::fs::{copy, read_link, read_to_string, OpenOptions};
use std::mem::MaybeUninit;
use std::path::{Path, PathBuf};

use libc::getuid;
use std::os::unix::io::AsRawFd;

use crate::{
    common::{call, file_exists, parse_file, path_append, MigErrCtx, MigError, MigErrorKind},
    defs::FileType,
    defs::{OSArch, DISK_BY_LABEL_PATH, DISK_BY_PARTUUID_PATH, DISK_BY_UUID_PATH},
    linux::linux_defs::{
        DF_CMD, FILE_CMD, KERNEL_CMDLINE_PATH, MKTEMP_CMD, MOKUTIL_CMD, NIX_NONE, SYS_UEFI_DIR,
        UNAME_CMD, WHEREIS_CMD,
    },
};

use crate::common::dir_exists;
use crate::common::stage2_config::BackupCfg;
use crate::defs::DeviceSpec;
use nix::{
    mount::{mount, MsFlags},
    sys::statvfs::statvfs,
};

use nix::{ioc, ioctl_none};

const BLK_IOC_MAGIC: u8 = 0x12;
const BLK_RRPART: u8 = 95;

ioctl_none!(blk_reread, BLK_IOC_MAGIC, BLK_RRPART);

const MOKUTIL_ARGS_SB_STATE: [&str; 1] = ["--sb-state"];

const UNAME_ARGS_OS_ARCH: [&str; 1] = ["-m"];

const BIN_DIRS: &[&str] = &["/bin", "/usr/bin", "/sbin", "/usr/sbin"];

const OS_RELEASE_FILE: &str = "/etc/os-release";
const OS_NAME_REGEX: &str = r#"^PRETTY_NAME="([^"]+)"$"#;

// file on ubuntu-14.04 reports x86 boot sector for image and kernel files

const OS_IMG_FTYPE_REGEX: &str = r#"^(DOS/MBR boot sector|x86 boot sector)$"#;
const GZIP_OS_IMG_FTYPE_REGEX: &str =
    r#"^(DOS/MBR boot sector|x86 boot sector).*\(gzip compressed data.*\)$"#;

const INITRD_FTYPE_REGEX: &str = r#"^ASCII cpio archive.*\(gzip compressed data.*\)$"#;
const OS_CFG_FTYPE_REGEX: &str = r#"^(ASCII text|JSON data).*$"#;
const KERNEL_AMD64_FTYPE_REGEX: &str =
    r#"^(Linux kernel x86 boot executable bzImage|x86 boot sector).*$"#;
const KERNEL_ARMHF_FTYPE_REGEX: &str = r#"^Linux kernel ARM boot executable zImage.*$"#;
//const KERNEL_I386_FTYPE_REGEX: &str = r#"^Linux kernel i386 boot executable bzImage.*$"#;
const KERNEL_AARCH64_FTYPE_REGEX: &str = r#"^MS-DOS executable.*$"#;

const TEXT_FTYPE_REGEX: &str = r#"^ASCII text.*$"#;

const DTB_FTYPE_REGEX: &str = r#"^(Device Tree Blob|data).*$"#;

const GZIP_TAR_FTYPE_REGEX: &str = r#"^(POSIX tar archive \(GNU\)).*\(gzip compressed data.*\)$"#;

//const MTAB_PATH: &str = "/etc/mtab";

pub(crate) fn is_admin() -> Result<bool, MigError> {
    trace!("LinuxMigrator::is_admin: entered");
    let admin = Some(unsafe { getuid() } == 0);
    Ok(admin.unwrap())
}

pub(crate) fn whereis(cmd: &str) -> Result<String, MigError> {
    // try manually first
    for path in BIN_DIRS {
        let path = format!("{}/{}", &path, cmd);
        if file_exists(&path) {
            return Ok(path);
        }
    }

    // else try whereis command
    let args: [&str; 2] = ["-b", cmd];
    let cmd_res = match call(WHEREIS_CMD, &args, true) {
        Ok(cmd_res) => cmd_res,
        Err(why) => {
            // manually try the usual suspects
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "whereis failed to execute for: {:?}, error: {:?}",
                    args, why
                ),
            ));
        }
    };

    if cmd_res.status.success() {
        if cmd_res.stdout.is_empty() {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("whereis: no command output for {}", cmd),
            ))
        } else {
            let mut words = cmd_res.stdout.split(' ');
            if let Some(s) = words.nth(1) {
                Ok(String::from(s))
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!("whereis: command not found: '{}'", cmd),
                ))
            }
        }
    } else {
        Err(MigError::from_remark(
            MigErrorKind::ExecProcess,
            &format!(
                "whereis: command failed for {}: {}",
                cmd,
                cmd_res.status.code().unwrap_or(0)
            ),
        ))
    }
}

pub(crate) fn get_os_arch() -> Result<OSArch, MigError> {
    trace!("get_os_arch: entered");
    let cmd_res = call(UNAME_CMD, &UNAME_ARGS_OS_ARCH, true).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("get_os_arch: call {}", UNAME_CMD),
    ))?;

    if cmd_res.status.success() {
        if cmd_res.stdout.to_lowercase() == "x86_64" {
            Ok(OSArch::AMD64)
        } else if cmd_res.stdout.to_lowercase() == "i386" {
            Ok(OSArch::I386)
        } else if cmd_res.stdout.to_lowercase() == "armv7l" {
            // TODO: try to determine the CPU Architecture
            Ok(OSArch::ARMHF)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("get_os_arch: unsupported architectute '{}'", cmd_res.stdout),
            ))
        }
    } else {
        Err(MigError::from_remark(
            MigErrorKind::ExecProcess,
            &format!("get_os_arch: command failed: {} {:?}", UNAME_CMD, cmd_res),
        ))
    }
}

pub(crate) fn get_mem_info() -> Result<(u64, u64), MigError> {
    trace!("get_mem_info: entered");
    // TODO: could add loads, uptime if needed
    let mut s_info: libc::sysinfo = unsafe { MaybeUninit::<libc::sysinfo>::zeroed().assume_init() };
    let res = unsafe { libc::sysinfo(&mut s_info) };
    if res == 0 {
        Ok((s_info.totalram as u64, s_info.freeram as u64))
    } else {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
}

/*
pub(crate) fn is_efi_boot() -> Result<bool, MigError> {
    trace!("is_efi_boot: entered");
    match std::fs::metadata(SYS_UEFI_DIR) {
        Ok(metadata) => Ok(metadata.file_type().is_dir()),
        Err(why) => match why.kind() {
            std::io::ErrorKind::NotFound => Ok(false),
            _ => Err(MigError::from(why.context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("is_uefi_boot: access {}", SYS_UEFI_DIR),
            )))),
        },
    }
}
*/

pub(crate) fn mktemp(
    dir: bool,
    pattern: Option<&str>,
    path: Option<PathBuf>,
) -> Result<PathBuf, MigError> {
    let mut cmd_args: Vec<&str> = Vec::new();

    let mut _dir_path: Option<String> = None;
    if let Some(path) = path {
        _dir_path = Some(String::from(path.to_string_lossy()));
        cmd_args.push("-p");
        cmd_args.push(_dir_path.as_ref().unwrap());
    }

    if dir {
        cmd_args.push("-d");
    }

    if let Some(pattern) = pattern {
        cmd_args.push(pattern);
    }

    let cmd_res = call(MKTEMP_CMD, cmd_args.as_slice(), true)?;

    if cmd_res.status.success() {
        Ok(PathBuf::from(cmd_res.stdout))
    } else {
        Err(MigError::from_remark(
            MigErrorKind::ExecProcess,
            &format!(
                "Failed to create temporary file for image extraction, error: {}",
                cmd_res.stderr
            ),
        ))
    }
}

pub(crate) fn tmp_mount<P: AsRef<Path>>(
    device: P,
    fs_type: &Option<String>,
    mount_dir: &Option<PathBuf>,
) -> Result<PathBuf, MigError> {
    // let no_path: Option<Path> = None;

    let fs_type = if let Some(fs_type) = fs_type {
        Some(fs_type.as_str().as_bytes())
    } else {
        None
    };

    let mount_dir = if let Some(mount_dir) = mount_dir {
        mount_dir.clone()
    } else {
        mktemp(true, None, None::<PathBuf>)?
    };

    mount(
        Some(device.as_ref()),
        &mount_dir,
        fs_type,
        MsFlags::empty(),
        NIX_NONE,
    )
    .context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!(
            "Failed to mount '{}' on '{}'",
            device.as_ref().display(),
            mount_dir.display()
        ),
    ))?;

    Ok(mount_dir)
}

/******************************************************************
 * Get OS name from /etc/os-release
 ******************************************************************/

pub(crate) fn get_os_name() -> Result<String, MigError> {
    trace!("get_os_name: entered");

    // TODO: implement other source as fallback

    if file_exists(OS_RELEASE_FILE) {
        // TODO: ensure availabilty of method / file exists
        if let Some(os_name) = parse_file(OS_RELEASE_FILE, &Regex::new(OS_NAME_REGEX).unwrap())? {
            Ok(os_name[1].clone())
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "get_os_name: could not be located in file {}",
                    OS_RELEASE_FILE
                ),
            ))
        }
    } else {
        Err(MigError::from_remark(
            MigErrorKind::NotFound,
            &format!("get_os_name: could not locate file {}", OS_RELEASE_FILE),
        ))
    }
}

/******************************************************************
 * Try to find out if secure boot is enabled using mokutil
 * assuming secure boot is not enabled if mokutil is absent
 ******************************************************************/

pub(crate) fn is_secure_boot() -> Result<bool, MigError> {
    trace!("is_secure_boot: entered");

    // TODO: check for efi vars

    if dir_exists(SYS_UEFI_DIR)? {
        let mokutil_path = match whereis(MOKUTIL_CMD) {
            Ok(path) => path,
            Err(_why) => {
                warn!("The mokutil command '{}' could not be found", MOKUTIL_CMD);
                return Ok(false);
            }
        };

        let cmd_res = call(&mokutil_path, &MOKUTIL_ARGS_SB_STATE, true)?;

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
            "is_secure_boot: failed to parse command output: '{}'",
            cmd_res.stdout
        );
        Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &"is_secure_boot: failed to parse command output".to_string(),
        ))
    } else {
        Ok(false)
    }
}

/******************************************************************
 * Restore a list of backed up files to a path
 * Backups are coded as list of ("orig-file","backup-file")
 ******************************************************************/

// TODO: allow restoring from work_dir to Boot

fn device_spec_to_path(
    dev_spec: DeviceSpec,
    fstype: &str,
    mount: bool,
    mount_dir: &Option<PathBuf>,
) -> Result<Option<PathBuf>, MigError> {
    trace!("device_spec_to_path: entered with {:?}", dev_spec);

    let dev_path = to_std_device_path(
        PathBuf::from(match dev_spec {
            DeviceSpec::DevicePath(ref path) => String::from(&*path.to_string_lossy()),
            DeviceSpec::Uuid(ref uuid) => format!("{}/{}", DISK_BY_UUID_PATH, uuid),
            DeviceSpec::Label(ref label) => format!("{}/{}", DISK_BY_LABEL_PATH, label),
            DeviceSpec::PartUuid(ref partuuid) => format!("{}/{}", DISK_BY_PARTUUID_PATH, partuuid),
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::NotImpl,
                    &format!("device_spec_to_path is not implemented for {:?}", dev_spec),
                ));
            }
        })
        .as_path(),
    )?;

    trace!(
        "device_spec_to_path: looking for mountpoint for: '{}'",
        dev_path.display()
    );

    let cmd_res = call(DF_CMD, &[&*dev_path.to_string_lossy()], true)?;
    let mounted_on = if cmd_res.status.success() {
        if let Some(line) = cmd_res.stdout.lines().nth(1) {
            trace!("device_spec_to_path: line 1: '{}'", line);
            if let Some(mountpoint) = line.split_whitespace().collect::<Vec<&str>>().get(5) {
                Some(PathBuf::from(mountpoint))
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvState,
                    &format!(
                        "Invalid output from df, cannot parse for mountpoint: '{}'",
                        cmd_res.stdout
                    ),
                ));
            }
        } else {
            None
        }
    } else {
        return Err(MigError::from_remark(
            MigErrorKind::InvState,
            &format!("Error from df: '{}'", cmd_res.stderr),
        ));
    };

    if mounted_on.is_some() {
        Ok(mounted_on)
    } else if mount {
        Ok(Some(tmp_mount(
            dev_path,
            &Some(String::from(fstype)),
            mount_dir,
        )?))
    } else {
        Ok(None)
    }
}

pub(crate) fn restore_backups(backups: &[BackupCfg], mount_dir: Option<PathBuf>) -> bool {
    // restore boot config backups
    debug!("restore_backups");
    let mut res = true;
    for backup in backups {
        if let Ok(mountpoint) =
            device_spec_to_path(backup.device.clone(), &backup.fstype, true, &mount_dir)
        {
            if let Some(mountpoint) = mountpoint {
                debug!(
                    "restore_backups: backup: '{}', mountpoint for '{:?}' is '{}'",
                    backup.backup.display(),
                    backup.device,
                    mountpoint.display()
                );
                let src = path_append(&mountpoint, &backup.backup);
                let tgt = path_append(&mountpoint, &backup.source);
                if let Err(why) = copy(&src, &tgt) {
                    error!(
                        "Failed to restore '{}' to '{}', error: {:?}",
                        src.display(),
                        tgt.display(),
                        why
                    );
                    res = false
                } else {
                    info!("Restored '{}' to '{}'", src.display(), tgt.display())
                }
            } else {
                error!("Failed to mount '{:?}'", backup.device,);
                res = false;
            }
        } else {
            error!(
                "Failed to retrieve mountpoint for backup: {:?}",
                backup.device
            );
            res = false;
        }
    }
    res
}

pub(crate) fn to_std_device_path(device: &Path) -> Result<PathBuf, MigError> {
    debug!("to_std_device_path: entered with '{}'", device.display());

    if !file_exists(device) {
        return Err(MigError::from_remark(
            MigErrorKind::NotFound,
            &format!("File does not exist: '{}'", device.display()),
        ));
    }

    if !(device.starts_with(DISK_BY_PARTUUID_PATH)
        || device.starts_with(DISK_BY_UUID_PATH)
        || device.starts_with(DISK_BY_LABEL_PATH))
    {
        return Ok(PathBuf::from(device));
    }

    trace!(
        "to_std_device_path: attempting to dereference as link '{}'",
        device.display()
    );

    match read_link(device) {
        Ok(link) => {
            if let Some(parent) = device.parent() {
                let dev_path = path_append(parent, link);
                Ok(dev_path.canonicalize().context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("failed to canonicalize path from: '{}'", dev_path.display()),
                ))?)
            } else {
                trace!("Failed to retrieve parent from  '{}'", device.display());
                Ok(PathBuf::from(device))
            }
        }
        Err(why) => {
            trace!(
                "Failed to dereference file '{}' : {:?}",
                device.display(),
                why
            );
            Ok(PathBuf::from(device))
        }
    }
}

pub(crate) fn drive_to_partition(drive: &Path, part_num: usize) -> Result<PathBuf, MigError> {
    const PART2DRIVE_REGEX: &str = r#"^(/dev/(([hs]d[a-z])|(nvme\d+n\d+|mmcblk\d+)))$"#;
    lazy_static! {
        static ref PART2DRIVE_RE: Regex = Regex::new(PART2DRIVE_REGEX).unwrap();
    }
    let path_str = String::from(&*drive.to_string_lossy());
    if let Some(ref captures) = PART2DRIVE_RE.captures(&path_str) {
        if captures.get(3).is_some() {
            Ok(PathBuf::from(format!("{}{}", path_str, part_num)))
        } else if captures.get(4).is_some() {
            Ok(PathBuf::from(format!("{}p{}", path_str, part_num)))
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "Failed to derive partition name from drive name: '{}'",
                    drive.display()
                ),
            ))
        }
    } else {
        Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!(
                "Failed to derive partition name from drive name: '{}'",
                drive.display()
            ),
        ))
    }
}

pub(crate) fn drive_from_partition(partition: &Path) -> Result<PathBuf, MigError> {
    const DRIVE2PART_REGEX: &str = r#"^(/dev/([hs]d[a-z]|nvme\d+n\d+|mmcblk\d+))(p?\d+)$"#;
    lazy_static! {
        static ref DRIVE2PART_RE: Regex = Regex::new(DRIVE2PART_REGEX).unwrap();
    }

    if let Some(captures) =
        DRIVE2PART_RE.captures(&to_std_device_path(partition)?.to_string_lossy())
    {
        Ok(PathBuf::from(captures.get(1).unwrap().as_str()))
    } else {
        Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!(
                "Failed to derive drive name from partition name: '{}'",
                partition.display()
            ),
        ))
    }
}

/*
pub(crate) fn get_os_release() -> Result<OSRelease, MigError> {
    let os_info =
        std::fs::read_to_string(OS_KERNEL_RELEASE_FILE).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("File read '{}'", OS_KERNEL_RELEASE_FILE),
        ))?;

    Ok(OSRelease::parse_from_str(&os_info.trim())?)
}
*/

pub(crate) fn get_fs_space<P: AsRef<Path>>(path: P) -> Result<(u64, u64), MigError> {
    let path = path.as_ref();
    let res = statvfs(path).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("Failed to retrieve stats for path: '{}'", path.display()),
    ))?;

    let blk_size = res.block_size() as u64;
    trace!(
        "statvfs('{}') returned size: {}, free: {}",
        path.display(),
        res.blocks() * blk_size,
        res.blocks_free() * blk_size
    );

    Ok((res.blocks() * blk_size, res.blocks_free() * blk_size))
}

/******************************************************************
* parse /proc/cmdline to extract root device & fs_type
******************************************************************/

pub(crate) fn get_kernel_root_info() -> Result<(PathBuf, Option<String>), MigError> {
    const ROOT_DEVICE_REGEX: &str = r#"\sroot=(\S+)\s"#;
    const ROOT_PARTUUID_REGEX: &str = r#"^PARTUUID=(\S+)$"#;
    const ROOT_UUID_REGEX: &str = r#"^UUID=(\S+)$"#;
    const ROOT_FSTYPE_REGEX: &str = r#"\srootfstype=(\S+)\s"#;

    trace!("get_root_info: entered");

    let cmd_line = read_to_string(KERNEL_CMDLINE_PATH).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("Failed to read from file: '{}'", KERNEL_CMDLINE_PATH),
    ))?;

    debug!("get_root_info: got cmdline: '{}'", cmd_line);

    let root_device = if let Some(captures) = RegexBuilder::new(ROOT_DEVICE_REGEX)
        .case_insensitive(true)
        .build()
        .unwrap()
        .captures(&cmd_line)
    {
        let root_dev = captures.get(1).unwrap().as_str();
        debug!("Got root device string: '{}'", root_dev);

        if let Some(uuid_part) = if let Some(captures) = RegexBuilder::new(ROOT_PARTUUID_REGEX)
            .case_insensitive(true)
            .build()
            .unwrap()
            .captures(root_dev)
        {
            debug!("Got root device PARTUUID: {:?}", captures.get(1));
            Some(path_append(
                DISK_BY_PARTUUID_PATH,
                captures.get(1).unwrap().as_str(),
            ))
        } else if let Some(captures) = RegexBuilder::new(ROOT_UUID_REGEX)
            .case_insensitive(true)
            .build()
            .unwrap()
            .captures(root_dev)
        {
            debug!("Got root device UUID: {:?}", captures.get(1));
            Some(path_append(
                DISK_BY_UUID_PATH,
                captures.get(1).unwrap().as_str(),
            ))
        } else {
            debug!("Got plain root device UUID: {:?}", captures.get(1));
            None
        } {
            debug!("trying device path: '{}'", uuid_part.display());
            uuid_part
        } else {
            debug!("Got plain root device '{}'", root_dev);
            PathBuf::from(root_dev)
        }
    } else {
        warn!(
            "Got no root was found in kernel command line '{}'",
            cmd_line
        );
        return Err(MigError::from_remark(
            MigErrorKind::NotFound,
            &format!(
                "Failed to parse root device path from kernel command line: '{}'",
                cmd_line
            ),
        ));
    };

    let root_device = to_std_device_path(&root_device)?;

    debug!("Using root device: '{}'", root_device.display());

    let root_fs_type = if let Some(captures) = RegexBuilder::new(&ROOT_FSTYPE_REGEX)
        .case_insensitive(true)
        .build()
        .unwrap()
        .captures(&cmd_line)
    {
        let fstype = captures.get(1).unwrap().as_str();
        debug!("Got root fstype: '{}'", fstype);
        Some(String::from(fstype))
    } else {
        warn!("failed to parse {} for root fs type", cmd_line);
        None
    };

    Ok((root_device, root_fs_type))
}

pub(crate) fn expect_type<P: AsRef<Path>>(file: P, ftype: &FileType) -> Result<(), MigError> {
    if !is_file_type(file.as_ref(), ftype)? {
        error!(
            "Could not determine expected file type '{}' for file '{}'",
            ftype.get_descr(),
            file.as_ref().display()
        );
        Err(MigError::displayed())
    } else {
        Ok(())
    }
}

pub(crate) fn is_file_type<P: AsRef<Path>>(file: P, ftype: &FileType) -> Result<bool, MigError> {
    if file.as_ref().exists() {
        let path_str = file.as_ref().to_string_lossy();
        let args: Vec<&str> = vec!["-bz", &path_str];

        let cmd_res = call(FILE_CMD, &args, true)?;
        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("new: failed determine type for file {}", path_str),
            ));
        }

        lazy_static! {
            static ref OS_IMG_FTYPE_RE: Regex = Regex::new(OS_IMG_FTYPE_REGEX).unwrap();
            static ref GZIP_OS_IMG_FTYPE_RE: Regex = Regex::new(GZIP_OS_IMG_FTYPE_REGEX).unwrap();
            static ref INITRD_FTYPE_RE: Regex = Regex::new(INITRD_FTYPE_REGEX).unwrap();
            static ref OS_CFG_FTYPE_RE: Regex = Regex::new(OS_CFG_FTYPE_REGEX).unwrap();
            static ref TEXT_FTYPE_RE: Regex = Regex::new(TEXT_FTYPE_REGEX).unwrap();
            static ref KERNEL_AMD64_FTYPE_RE: Regex = Regex::new(KERNEL_AMD64_FTYPE_REGEX).unwrap();
            static ref KERNEL_ARMHF_FTYPE_RE: Regex = Regex::new(KERNEL_ARMHF_FTYPE_REGEX).unwrap();
            static ref KERNEL_AARCH64_FTYPE_RE: Regex = Regex::new(KERNEL_AARCH64_FTYPE_REGEX).unwrap();
            //static ref KERNEL_I386_FTYPE_RE: Regex = Regex::new(KERNEL_I386_FTYPE_REGEX).unwrap();
            static ref DTB_FTYPE_RE: Regex = Regex::new(DTB_FTYPE_REGEX).unwrap();
            static ref GZIP_TAR_FTYPE_RE: Regex = Regex::new(GZIP_TAR_FTYPE_REGEX).unwrap();
        }

        debug!(
            "FileInfo::is_type: looking for: {}, found {}",
            ftype.get_descr(),
            cmd_res.stdout
        );
        match ftype {
            FileType::GZipOSImage => Ok(GZIP_OS_IMG_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::OSImage => Ok(OS_IMG_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::InitRD => Ok(INITRD_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::KernelARMHF => Ok(KERNEL_ARMHF_FTYPE_RE.is_match(&cmd_res.stdout)),
            //FileType::KernelAMD64 => Ok(KERNEL_AMD64_FTYPE_RE.is_match(&cmd_res.stdout)),
            //FileType::KernelI386 => Ok(KERNEL_I386_FTYPE_RE.is_match(&cmd_res.stdout)),
            //FileType::KernelAARCH64 => Ok(KERNEL_AARCH64_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::Json => Ok(OS_CFG_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::Text => Ok(TEXT_FTYPE_RE.is_match(&cmd_res.stdout)),
            // FileType::DTB => Ok(DTB_FTYPE_RE.is_match(&cmd_res.stdout)),
            FileType::GZipTar => Ok(GZIP_TAR_FTYPE_RE.is_match(&cmd_res.stdout)),
        }
    } else {
        Err(MigError::from_remark(
            MigErrorKind::NotFound,
            &format!("File could not be found: '{}'", file.as_ref().display()),
        ))
    }
}

pub fn ioc_part_reread(device: &Path) -> Result<i32, MigError> {
    // try ioctrl #define BLKRRPART  _IO(0x12,95)	/* re-read partition table */
    match OpenOptions::new()
        .read(true)
        .write(true)
        .create(false)
        .open(device)
    {
        Ok(file) => match unsafe { blk_reread(file.as_raw_fd()) } {
            Ok(res) => {
                debug!(
                    "Device BLKRRPART IOCTRL to '{}' returned {}",
                    device.display(),
                    res
                );
                Ok(res)
            }
            Err(why) => {
                error!(
                    "Device BLKRRPART IOCTRL to '{}' failed with error: {:?}",
                    device.display(),
                    why
                );
                Err(MigError::displayed())
            }
        },
        Err(why) => {
            error!(
                "Failed to open device '{}', error: {:?}",
                device.display(),
                why
            );
            Err(MigError::displayed())
        }
    }
}
