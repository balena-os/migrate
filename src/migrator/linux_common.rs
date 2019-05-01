use failure::{Fail, ResultExt};

use log::{error, trace, warn};
use regex::Regex;
use std::collections::HashMap;
use std::cell::{RefCell};

use libc::getuid;

use crate::common::{
    call, 
    file_exists,
    parse_file,
    CmdRes, 
    Config,
    OSArch,
    MigErrCtx, 
    MigError, 
    MigErrorKind,
};

pub(crate) mod disk_info;
pub(crate) use disk_info::DiskInfo;

pub(crate) mod migrate_info;
pub(crate) use migrate_info::MigrateInfo;

pub(crate) mod path_info;
pub(crate) use path_info::PathInfo;

const MODULE: &str = "balena-migrate::linux_common";
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


const UNAME_ARGS_OS_ARCH: [&str; 1] = ["-m"];

// TODO: make this more complete
const BIN_DIRS: &[&str] = &["/bin", "/usr/bin", "/sbin" ];

// const OS_KERNEL_RELEASE_FILE: &str = "/proc/sys/kernel/osrelease";
// const OS_MEMINFO_FILE: &str = "/proc/meminfo";

const OS_RELEASE_FILE: &str = "/etc/os-release";
const OS_NAME_REGEX: &str = r#"^PRETTY_NAME="([^"]+)"$"#;

const SYS_UEFI_DIR: &str = "/sys/firmware/efi";

thread_local! {
    static CMD_TABLE: RefCell<HashMap<String,Option<String>>> = RefCell::new(HashMap::new());
}

pub(crate) fn ensure_cmds(required: &[&str], optional: &[&str]) -> Result<(),MigError> {
    CMD_TABLE.with(|cmd_tbl| {
        let mut cmd_table = cmd_tbl.borrow_mut();
        for cmd in required {
            if let Ok(cmd_path) = whereis(cmd) {
                cmd_table.insert(String::from(*cmd),Some(cmd_path));                    
            } else {
                let message = format!("cannot find required command {}", cmd);
                error!("{}", message);
                return Err(MigError::from_remark(MigErrorKind::NotFound, &format!("{}", message)));
            }
        }

        for cmd in optional {
            match  whereis(cmd) {
                Ok(cmd_path) => {
                    cmd_table.insert(String::from(*cmd),Some(cmd_path));
                    ()
                },
                Err(_why) => {
                    // TODO: forward upstream error message
                    let message = format!("cannot find optional command {}", cmd);
                    warn!("{}", message);
                    cmd_table.insert(String::from(*cmd),None);
                    ()                  
                }, 
            }
        }
        Ok(())                
    })
}

fn get_cmd(cmd: &str) -> Result<String,MigError> {
    CMD_TABLE.with(|cmd_tbl| {
        match cmd_tbl.borrow().get(cmd) {
            Some(cmd_path) => match cmd_path {
                Some(cmd_path) => Ok(cmd_path.clone()),
                None => Err(MigError::from_remark(MigErrorKind::NotFound, &format!("The command was not found: {}", cmd))),
            } ,
            None => Err(MigError::from_remark(MigErrorKind::InvParam, &format!("The command is not a checked command: {}", cmd))),
        }
    })
}

pub(crate) fn call_cmd(cmd: &str, args: &[&str], trim_stdout: bool) -> Result<CmdRes, MigError> {
    trace!(
        "call_cmd: entered with cmd: '{}', args: {:?}, trim: {}",
        cmd,
        args,
        trim_stdout
    );

    Ok(call(&get_cmd(cmd)?, args, trim_stdout)?)
}

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
    Ok(admin.unwrap() | config.debug.fake_admin)
}


/*
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
*/


fn whereis(cmd: &str) -> Result<String, MigError> {
    let args: [&str; 2] = ["-b", cmd];
    let cmd_res = match call(WHEREIS_CMD, &args, true) {
        Ok(cmd_res) => cmd_res,
        Err(_why) => {            
            // manually try the usual suspects
            for path in BIN_DIRS {
                let path = format!("{}/{}", &path, cmd);
                if file_exists(&path) {
                    return Ok(path);
                }
            }
            return Err(MigError::from_remark(MigErrorKind::NotFound, &format!("could not find command: '{}'", cmd)));
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


pub(crate) fn get_os_name() -> Result<String, MigError> {
    trace!("get_os_name: entered");
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