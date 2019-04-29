use lazy_static::lazy_static;
use std::collections::HashMap;
use log::{error, trace, debug};
use regex::Regex;

use failure::{ResultExt};

use crate::common::{MigError,MigErrCtx, MigErrorKind, OSArch, CmdRes};
use crate::linux_common::{call_cmd_from, whereis};

const GRUB_INST_VERSION_ARGS: [&str; 1] = ["--version"];
const GRUB_INST_VERSION_RE: &str = r#"^.*\s+\(GRUB\)\s+([0-9]+)\.([0-9]+)[^0-9].*$"#;

const UNAME_ARGS_OS_ARCH: [&str; 1] = ["-m"];
const MOKUTIL_ARGS_SB_STATE: [&str; 1] = ["--sb-state"];

pub const DF_CMD: &str = "df";
pub const LSBLK_CMD: &str = "lsblk";
pub const MOUNT_CMD: &str = "mount";
pub const FILE_CMD: &str = "file";
pub const UNAME_CMD: &str = "uname";
pub const MOKUTIL_CMD: &str = "mokutil";
pub const GRUB_INSTALL_CMD: &str = "grub-install";
pub const REBOOT_CMD: &str = "reboot";
pub const CHMOD_CMD: &str = "chmod";

const REQUIRED_CMDS: &'static [&'static str] = &[DF_CMD, LSBLK_CMD, MOUNT_CMD, FILE_CMD, UNAME_CMD, REBOOT_CMD, CHMOD_CMD];
const OPTIONAL_CMDS: &'static [&'static str] = &[MOKUTIL_CMD, GRUB_INSTALL_CMD];

const MODULE: &str = "balena_migrator::linux::util";

pub(crate) fn call_cmd(cmd: &str, args: &[&str], trim_stdout: bool) -> Result<CmdRes, MigError> {
    lazy_static! {
        static ref CMD_PATH: HashMap<String,Option<String>> = {
            let mut map = HashMap::new();
            for cmd in REQUIRED_CMDS {
                map.insert(
                    String::from(*cmd),
                    Some(match whereis(cmd) {
                        Ok(cmd) => cmd,
                        Err(_why) => {
                            let message = format!("cannot find required command {}", cmd);
                            error!("{}", message);
                            panic!("{}", message);
                        }
                    }));
            }
            for cmd in OPTIONAL_CMDS {
                map.insert(
                    String::from(*cmd),
                    match whereis(cmd) {
                        Ok(cmd) => Some(cmd),
                        Err(_why) => None, // TODO: check error codes
                    });
            }
            map
        };
    }

    call_cmd_from(&CMD_PATH, cmd, args, trim_stdout)
}

pub(crate) fn get_os_arch() -> Result<OSArch, MigError> {
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

pub(crate) fn get_grub_version() -> Result<(String, String), MigError> {
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

pub(crate) fn is_secure_boot() -> Result<bool, MigError> {
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
