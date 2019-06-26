use failure::ResultExt;

use lazy_static::lazy_static;
use log::{debug, error, info, trace, warn};
use regex::{Regex, RegexBuilder};
use std::fs::{copy, read_link, read_to_string};
use std::path::{Path, PathBuf};

use libc::getuid;

use crate::{
    common::{
        call, file_exists, parse_file, path_append, Config, MigErrCtx, MigError, MigErrorKind,
    },
    defs::{OSArch, DISK_BY_LABEL_PATH, DISK_BY_PARTUUID_PATH, DISK_BY_UUID_PATH},
    linux::{
        linux_defs::{KERNEL_CMDLINE_PATH, SYS_UEFI_DIR},
        EnsuredCmds, DF_CMD, MKTEMP_CMD, MOKUTIL_CMD, UNAME_CMD,
    },
};

use crate::common::dir_exists;

const MODULE: &str = "linux_common";
const WHEREIS_CMD: &str = "whereis";

const MOKUTIL_ARGS_SB_STATE: [&str; 1] = ["--sb-state"];

const UNAME_ARGS_OS_ARCH: [&str; 1] = ["-m"];

const BIN_DIRS: &[&str] = &["/bin", "/usr/bin", "/sbin", "/usr/sbin"];

const OS_RELEASE_FILE: &str = "/etc/os-release";
const OS_NAME_REGEX: &str = r#"^PRETTY_NAME="([^"]+)"$"#;

const DRIVE2PART_REGEX: &str = r#"^(/dev/([hs]d[a-z]|nvme\d+n\d+|mmcblk\d+))(p?\d+)$"#;

#[cfg(not(debug_assertions))]
pub(crate) fn is_admin(_config: &Config) -> Result<bool, MigError> {
    trace!("LinuxMigrator::is_admin: entered");
    let admin = Some(unsafe { getuid() } == 0);
    Ok(admin.unwrap())
}

#[cfg(debug_assertions)]
pub(crate) fn is_admin(config: &Config) -> Result<bool, MigError> {
    trace!("LinuxMigrator::is_admin: entered");
    let admin = Some(unsafe { getuid() } == 0);
    Ok(admin.unwrap() | config.debug.is_fake_admin())
}

pub(crate) fn whereis(cmd: &str) -> Result<String, MigError> {
    // try manually first
    for path in BIN_DIRS {
        let path = format!("{}/{}", &path, cmd);
        if file_exists(&path) {
            return Ok(path);
        }
    }

    // else try wheris command
    let args: [&str; 2] = ["-b", cmd];
    let cmd_res = match call(WHEREIS_CMD, &args, true) {
        Ok(cmd_res) => cmd_res,
        Err(_why) => {
            // manually try the usual suspects
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("could not find command: '{}'", cmd),
            ));
        }
    };

    if cmd_res.status.success() {
        if cmd_res.stdout.is_empty() {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("{}::whereis: no command output for {}", MODULE, cmd),
            ))
        } else {
            let mut words = cmd_res.stdout.split(" ");
            if let Some(s) = words.nth(1) {
                Ok(String::from(s))
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!("{}::whereis: command not found: '{}'", MODULE, cmd),
                ))
            }
        }
    } else {
        Err(MigError::from_remark(
            MigErrorKind::ExecProcess,
            &format!(
                "{}::whereis: command failed for {}: {}",
                MODULE,
                cmd,
                cmd_res.status.code().unwrap_or(0)
            ),
        ))
    }
}

pub(crate) fn get_os_arch(cmds: &EnsuredCmds) -> Result<OSArch, MigError> {
    trace!("get_os_arch: entered");
    let cmd_res =
        cmds.call(UNAME_CMD, &UNAME_ARGS_OS_ARCH, true)
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("get_os_arch: call {}", UNAME_CMD),
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
                "{}::get_os_arch: command failed: {} {:?}",
                MODULE, UNAME_CMD, cmd_res
            ),
        ))
    }
}

pub(crate) fn get_mem_info() -> Result<(u64, u64), MigError> {
    trace!("get_mem_info: entered");
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

/*
pub(crate) fn is_efi_boot() -> Result<bool, MigError> {
    trace!("is_efi_boot: entered");
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
*/

pub(crate) fn mktemp<P: AsRef<Path>>(
    cmds: &EnsuredCmds,
    dir: bool,
    pattern: Option<&str>,
    path: Option<P>,
) -> Result<PathBuf, MigError> {
    let mut cmd_args: Vec<&str> = Vec::new();

    let mut dir_path: Option<String> = None;
    if let Some(path) = path {
        dir_path = Some(String::from(path.as_ref().to_string_lossy()));
        cmd_args.push("-p");
        cmd_args.push(dir_path.as_ref().unwrap());
    }

    if dir {
        cmd_args.push("-d");
    }

    if let Some(pattern) = pattern {
        cmd_args.push(pattern);
    }

    let cmd_res = cmds.call(MKTEMP_CMD, cmd_args.as_slice(), true)?;

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

/******************************************************************
 * Try to find out if secure boot is enabled using mokutil
 * assuming secure boot is not enabled if mokutil is absent
 ******************************************************************/

pub(crate) fn is_secure_boot() -> Result<bool, MigError> {
    trace!("{}::is_secure_boot: entered", MODULE);

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
            "{}::is_secure_boot: failed to parse command output: '{}'",
            MODULE, cmd_res.stdout
        );
        Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!("{}::is_secure_boot: failed to parse command output", MODULE),
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

pub(crate) fn restore_backups(
    root_path: &Path,
    backups: &[(String, String)],
) -> Result<(), MigError> {
    // restore boot config backups
    for backup in backups {
        let src = path_append(root_path, &backup.1);
        let tgt = path_append(root_path, &backup.0);
        copy(&src, &tgt).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Failed to restore '{}' to '{}'",
                src.display(),
                tgt.display()
            ),
        ))?;
        info!("Restored '{}' to '{}'", src.display(), tgt.display())
    }

    Ok(())
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

    debug!(
        "to_std_device_path: attempting to dereference as link '{}'",
        device.display()
    );

    match read_link(device) {
        Ok(link) => {
            if let Some(parent) = device.parent() {
                let dev_path = path_append(parent, link);
                return Ok(dev_path.canonicalize().context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("failed to canonicalize path from: '{}'", dev_path.display()),
                ))?);
            } else {
                debug!("Failed to retrieve parent from  '{}'", device.display());
                return Ok(PathBuf::from(device));
            }
        }
        Err(why) => {
            debug!(
                "Failed to dereference file '{}' : {:?}",
                device.display(),
                why
            );
            return Ok(PathBuf::from(device));
        }
    }
}

pub(crate) fn drive_from_partition(partition: &Path) -> Result<PathBuf, MigError> {
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

pub(crate) fn get_fs_space<P: AsRef<Path>>(
    cmds: &EnsuredCmds,
    path: P,
) -> Result<(u64, u64), MigError> {
    const SIZE_REGEX: &str = r#"^(\d+)K?$"#;
    let path = path.as_ref();
    trace!("get_fs_space: entered with '{}'", path.display());

    let path_str = path.to_string_lossy();
    let args: Vec<&str> = vec!["--block-size=K", "--output=size,used", &path_str];

    let cmd_res = cmds.call(DF_CMD, &args, true)?;

    if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
        return Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!(
                "get_fs_space: failed to get drive space for path '{}'",
                path.display()
            ),
        ));
    }

    let output: Vec<&str> = cmd_res.stdout.lines().collect();
    if output.len() != 2 {
        return Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!(
                "get_fs_space: failed to parse df output attributes for '{}'",
                path.display()
            ),
        ));
    }

    // debug!("PathInfo::new: '{}' df result: {:?}", path, &output[1]);

    let words: Vec<&str> = output[1].split_whitespace().collect();
    if words.len() != 2 {
        debug!(
            "get_fs_space: '{}' df result: words {}",
            path.display(),
            words.len()
        );
        return Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!(
                "get_fs_space: failed to parse df output for {}",
                path.display()
            ),
        ));
    }

    debug!("get_fs_space: '{}' df result: {:?}", path.display(), &words);

    lazy_static! {
        static ref SIZE_RE: Regex = Regex::new(SIZE_REGEX).unwrap();
    }

    let fs_size = if let Some(captures) = SIZE_RE.captures(words[0]) {
        captures
            .get(1)
            .unwrap()
            .as_str()
            .parse::<u64>()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("get_fs_space: failed to parse size from {} ", words[0]),
            ))?
            * 1024
    } else {
        return Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!("get_fs_space: failed to parse size from {} ", words[0]),
        ));
    };

    let fs_used = if let Some(captures) = SIZE_RE.captures(words[1]) {
        captures
            .get(1)
            .unwrap()
            .as_str()
            .parse::<u64>()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("get_fs_space: failed to parse size from {} ", words[1]),
            ))?
            * 1024
    } else {
        return Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!("get_fs_space: failed to parse size from {} ", words[1]),
        ));
    };

    Ok((fs_size, fs_size - fs_used))
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
        } else {
            if let Some(captures) = RegexBuilder::new(ROOT_UUID_REGEX)
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
            }
        } {
            debug!("trying device path: '{}'", uuid_part.display());
            to_std_device_path(&uuid_part)?
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

    debug!("Using root device: '{}'", root_device.display());

    let root_fs_type =
        if let Some(captures) = Regex::new(&ROOT_FSTYPE_REGEX).unwrap().captures(&cmd_line) {
            Some(String::from(captures.get(1).unwrap().as_str()))
        } else {
            warn!("failed to parse {} for root fs type", cmd_line);
            None
        };

    Ok((root_device, root_fs_type))
}
