use failure::{Fail,ResultExt};
use log::debug;
use regex::{Regex};
use std::fs::{metadata, read_to_string};
use lazy_static::lazy_static;
use log::{error, trace};
use std::collections::HashMap;
use std::path::Path;

use libc::{getuid};

const MODULE: &str = "Linux::util";
const WHEREIS_CMD: &str = "whereis";

pub const DF_CMD: &str = "df";
pub const LSBLK_CMD: &str = "lsblk";
pub const MOUNT_CMD: &str = "mount";
pub const FILE_CMD: &str = "file";
pub const UNAME_CMD: &str = "uname";
pub const MOKUTIL_CMD: &str = "mokutil";
pub const GRUB_INSTALL_CMD: &str = "grub-install";


const GRUB_INST_VERSION_ARGS: [&str; 1] = ["--version"];
const GRUB_INST_VERSION_RE: &str = r#"^.*\s+\(GRUB\)\s+([0-9]+)\.([0-9]+)[^0-9].*$"#;


const UNAME_ARGS_OS_ARCH: [&str; 1] = ["-m"];
const MOKUTIL_ARGS_SB_STATE: [&str; 1] = ["--sb-state"];

const REQUIRED_CMDS: &'static [&'static str] = &[DF_CMD, LSBLK_CMD, MOUNT_CMD, FILE_CMD, UNAME_CMD];

const OPTIONAL_CMDS: &'static [&'static str] = &[MOKUTIL_CMD, GRUB_INSTALL_CMD];

const OS_KERNEL_RELEASE_FILE: &str = "/proc/sys/kernel/osrelease";
const OS_MEMINFO_FILE: &str = "/proc/meminfo";

const OS_RELEASE_FILE: &str = "/etc/os-release";
const OS_NAME_REGEX: &str = r#"^PRETTY_NAME="([^"]+)"$"#;

const SYS_UEFI_DIR: &str = "/sys/firmware/efi";


use crate::migrator::{
    common::{call, CmdRes, OSArch, OSRelease},
    MigErrCtx, MigError, MigErrorKind,
};

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



pub(crate) fn call_cmd(cmd: &str, args: &[&str], trim_stdout: bool) -> Result<CmdRes, MigError> {
    lazy_static! {
        static ref CMD_PATH: HashMap<String,Option<String>> = {
            let mut map = HashMap::new();
            for cmd in REQUIRED_CMDS {
                map.insert(
                    String::from(*cmd),
                    Some(match whereis(cmd) {
                        Ok(cmd) => cmd,
                        Err(why) => {
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

    trace!(
        "call_cmd: entered with cmd: '{}', args: {:?}, trim: {}",
        cmd,
        args,
        trim_stdout
    );
    if let Some(found_cmd) = CMD_PATH.get(cmd) {
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

#[derive(Debug)]
pub struct FileInfo {
    pub path: String,
    pub ftype: String,
    pub size: u64,
    pub in_work_dir: bool,
}

impl FileInfo {
    pub fn default(path: &str, size: u64) -> FileInfo {
        FileInfo {
            path: String::from(path),
            ftype: String::from(""),
            size,
            in_work_dir: false,
        }
    }
}

pub(crate) fn get_file_info(file: &str, work_dir: &str) -> Result<Option<FileInfo>, MigError> {
    trace!(
        "{}::check_work_file: entered with file: '{}', work_dir: '{}'",
        MODULE,
        file,
        work_dir
    );

    let checked_path = if file.starts_with("/") || file.starts_with("./") || file.starts_with("../")
    {
        if let Ok(mdata) = metadata(file) {
            Some(FileInfo::default(
                &std::fs::canonicalize(Path::new(file))
                    .unwrap()
                    .to_str()
                    .unwrap(),
                mdata.len(),
            ))
        } else {
            None
        }
    } else {
        let search = if work_dir.ends_with("/") {
            format!("{}{}", work_dir, file)
        } else {
            format!("{}/{}", work_dir, file)
        };

        if let Ok(mdata) = metadata(&search) {
            Some(FileInfo::default(
                &std::fs::canonicalize(Path::new(&search))
                    .unwrap()
                    .to_str()
                    .unwrap(),
                mdata.len(),
            ))
        } else {
            None
        }
    };

    debug!(
        "{}::check_work_file: checked path for '{}': '{:?}'",
        MODULE, file, &checked_path
    );

    if let Some(mut file_info) = checked_path {
        file_info.in_work_dir = file_info.path.starts_with(work_dir);

        let args: Vec<&str> = vec!["-bz", &file_info.path];
        let cmd_res = call_cmd(FILE_CMD, &args, true)?;
        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::new: failed determine type for file {}",
                    MODULE, &file_info.path
                ),
            ));
        }
        file_info.ftype = String::from(cmd_res.stdout);
        Ok(Some(file_info))
    } else {
        Ok(None)
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

pub fn expect_file(
    file: &str,
    descr: &str,
    expected: &str,
    work_dir: &str,
    type_regex: &Regex,
) -> Result<Option<FileInfo>, MigError> {
    if !file.is_empty() {
        if let Some(file_info) = get_file_info(&file, work_dir)? {
            debug!("{} -> {:?}", file, &file_info);
            if !type_regex.is_match(&file_info.ftype) {
                let message = format!(
                    "{} '{}' is in an invalid format, expected {}, got {}",
                    descr, &file, expected, &file_info.ftype
                );
                error!("{}", message);
                return Err(MigError::from_remark(MigErrorKind::InvParam, &message));
            }

            Ok(Some(file_info))
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
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

pub(crate) fn get_os_name() -> Result<String, MigError> {
    trace!("LinuxMigrator::get_os_name: entered");
    if file_exists(OS_RELEASE_FILE) {
        // TODO: ensure availabilty of method / file exists
        if let Some(os_name) =
            parse_file(OS_RELEASE_FILE, &Regex::new(OS_NAME_REGEX).unwrap())?
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

pub(crate) fn get_os_release() -> Result<OSRelease, MigError> {
    let os_info =
        std::fs::read_to_string(OS_KERNEL_RELEASE_FILE).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("File read '{}'", OS_KERNEL_RELEASE_FILE),
        ))?;

    Ok(OSRelease::parse_from_str(&os_info.trim())?)
}

pub fn command_exists(cmd: &str) -> Result<bool, MigError> {
    Err(MigError::from(MigErrorKind::NotImpl))
}

pub fn exec_command(cmd: &str) -> Result<bool, MigError> {
    Err(MigError::from(MigErrorKind::NotImpl))
}
