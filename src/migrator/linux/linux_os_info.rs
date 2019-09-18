use std::path::{Path};
use libc::getuid;
use log::{trace};
use failure::{ResultExt};
use regex::{Regex};

use crate::{
    common::{file_exists, parse_file,
             os_info::{OSInfo, PathInfo},
             mig_error::{MigError, MigErrCtx, MigErrorKind},
             file_info::FileType},
    defs::{OSArch},
    linux::ensured_cmds::{
        EnsuredCmds, CHMOD_CMD, DF_CMD, FILE_CMD, GRUB_REBOOT_CMD, GRUB_UPDT_CMD, LSBLK_CMD,
        MKTEMP_CMD, MOKUTIL_CMD, MOUNT_CMD, REBOOT_CMD, TAR_CMD, UNAME_CMD,
    },
};

const OS_RELEASE_FILE: &str = "/etc/os-release";

pub struct LinuxOSInfo {
    cmds: EnsuredCmds,
}

impl LinuxOSInfo {
    pub fn new(req_cmds: &[&str]) -> Result<LinuxOSInfo, MigError> {
        let mut cmds = EnsuredCmds::new();
        cmds.ensure_cmds(req_cmds)?;
        // TODO: add cmds required by LinuxOSInfo
        Ok(LinuxOSInfo{ cmds })
    }
}

impl OSInfo for LinuxOSInfo {
    fn is_admin(&self) -> Result<bool, MigError> {
        trace!("linux_os_info::is_admin: entered");
        let admin = Some(unsafe { getuid() } == 0);
        Ok(admin.unwrap())
    }

    fn get_os_arch(&self) -> Result<OSArch, MigError> {
        trace!("get_os_arch: entered");
        let cmd_res =
            self.cmds.call(UNAME_CMD, &["-m"], true)
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
                        "linux_os_info::get_os_arch: unsupported architectute '{}'",
                        cmd_res.stdout
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

    fn get_os_name(&self) -> Result<String, MigError> {
        /******************************************************************
         * Get OS name from /etc/os-release
         ******************************************************************/

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

    // TODO: call command interface incl ensured commands

    fn get_mem_info(&self) -> Result<(u64, u64), MigError> {
        trace!("get_mem_info: entered");
        // TODO: could add loads, uptime if needed
        use std::mem;
        let mut s_info: libc::sysinfo = unsafe { mem::MaybeUninit() };
        let res = unsafe { libc::sysinfo(&mut s_info) };
        if res == 0 {
            Ok((s_info.totalram as u64, s_info.freeram as u64))
        } else {
            Err(MigError::from(MigErrorKind::NotImpl))
        }
    }

    // Disk specific calls
    fn get_path_info(&self, path: &Path) -> Result<PathInfo, MigError> {
        unimplemented!()
    }
    fn get_boot_info(&self) -> Result<PathInfo, MigError> {
        unimplemented!()
    }
    fn get_install_drive_info(&self) -> Result<PathInfo, MigError> {
        unimplemented!()
    }

    // file types
    fn is_file_type(&self, ftype: FileType) -> Result<bool, MigError> {
        unimplemented!()
    }
}