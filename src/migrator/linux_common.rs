use failure::{Fail, ResultExt};

use log::debug;
use log::{error, trace};
use regex::Regex;
use std::collections::HashMap;
use std::fs::read_to_string;
use std::path::Path;

use libc::getuid;

use crate::common::{
    call, 
    CmdRes, 
    MigErrCtx, 
    MigError, 
    MigErrorKind,
};


const MODULE: &str = "Linux::util";
const WHEREIS_CMD: &str = "whereis";

pub const DF_CMD: &str = "df";
pub const LSBLK_CMD: &str = "lsblk";
pub const MOUNT_CMD: &str = "mount";
pub const FILE_CMD: &str = "file";
pub const UNAME_CMD: &str = "uname";
pub const MOKUTIL_CMD: &str = "mokutil";
pub const GRUB_INSTALL_CMD: &str = "grub-install";
pub const REBOOT_CMD: &str = "reboot";
pub const CHMOD_CMD: &str = "chmod";

// const OS_KERNEL_RELEASE_FILE: &str = "/proc/sys/kernel/osrelease";
// const OS_MEMINFO_FILE: &str = "/proc/meminfo";

const OS_RELEASE_FILE: &str = "/etc/os-release";
const OS_NAME_REGEX: &str = r#"^PRETTY_NAME="([^"]+)"$"#;

const SYS_UEFI_DIR: &str = "/sys/firmware/efi";


#[cfg(not(debug_assertions))]
pub(crate) fn is_admin(_fake_admin: bool) -> Result<bool, MigError> {
    trace!("LinuxMigrator::is_admin: entered");
    let admin = Some(unsafe { getuid() } == 0);
    Ok(admin.unwrap())
}

#[cfg(debug_assertions)]
pub(crate) fn is_admin(fake_admin: bool) -> Result<bool, MigError> {
    trace!("LinuxMigrator::is_admin: entered");
    let admin = Some(unsafe { getuid() } == 0);
    Ok(admin.unwrap() | fake_admin)
}

pub(crate) fn call_cmd_from(list: &HashMap<String,Option<String>>, cmd: &str, args: &[&str], trim_stdout: bool) -> Result<CmdRes, MigError> {
    trace!(
        "call_cmd: entered with cmd: '{}', args: {:?}, trim: {}",
        cmd,
        args,
        trim_stdout
    );
    if let Some(found_cmd) = list.get(cmd) {
        if let Some(valid_cmd) = found_cmd {
            Ok(call(valid_cmd, args, trim_stdout)?)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("{}::call_cmd: {} is not available", MODULE, cmd),
            ))
        }
    } else {
        Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!(
                "{}::call_cmd: {} is not in the list of checked commands",
                MODULE, cmd
            ),
        ))
    }
}

pub fn parse_file(fname: &str, regex: &Regex) -> Result<Option<Vec<String>>, MigError> {
    let os_info = read_to_string(fname).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("File read '{}'", fname),
    ))?;

    for line in os_info.lines() {
        debug!("{}::parse_file: line: '{}'", MODULE, line);

        if let Some(ref captures) = regex.captures(line) {
            let mut results: Vec<String> = Vec::new();
            for cap in captures.iter() {
                if let Some(cap) = cap {
                    results.push(String::from(cap.as_str()));
                } else {
                    results.push(String::from(""));
                }
            }
            return Ok(Some(results));
        };
    }

    Ok(None)
}

pub fn dir_exists(name: &str) -> Result<bool, MigError> {
    let path = Path::new(name);
    if path.exists() {
        Ok(path
            .metadata()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "{}::dir_exists: failed to retrieve metadata for path: {}",
                    MODULE, name
                ),
            ))?
            .file_type()
            .is_dir())
    } else {
        Ok(false)
    }
}

pub fn file_exists(file: &str) -> bool {
    Path::new(file).exists()
}

pub fn whereis(cmd: &str) -> Result<String, MigError> {
    let args: [&str; 2] = ["-b", cmd];
    let cmd_res = call(WHEREIS_CMD, &args, true).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("{}::whereis: failed for '{}'", MODULE, cmd),
    ))?;
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
                //
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

pub(crate) fn get_mem_info() -> Result<(u64, u64), MigError> {
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


pub(crate) fn is_uefi_boot() -> Result<bool, MigError> {
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


pub(crate) fn get_os_name() -> Result<String, MigError> {
    trace!("LinuxMigrator::get_os_name: entered");
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