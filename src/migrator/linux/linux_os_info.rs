use failure::ResultExt;
use lazy_static::lazy_static;
use libc::getuid;
use log::{debug, trace};
use regex::Regex;
use std::path::{Path, PathBuf};

use crate::{
    common::{
        file_exists,
        file_info::FileType,
        mig_error::{MigErrCtx, MigError, MigErrorKind},
        os_info::{OSInfo, PathInfo},
        parse_file,
    },
    defs::OSArch,
    linux::{
        ensured_cmds::{EnsuredCmds, DF_CMD, UNAME_CMD},
        linux_common::get_kernel_root_info,
    },
};

mod lsblk_info;
use lsblk_info::{LsblkInfo, LsblkPartition, LsblkDevice};

mod ensured_cmds;

pub(crate) mod linux_path_info;
use linux_path_info::LinuxPathInfo;

const OS_RELEASE_FILE: &str = "/etc/os-release";
const OS_NAME_REGEX: &str = r#"^PRETTY_NAME="([^"]+)"$"#;
const ROOT_PATH: &str = "/";


pub struct LinuxOSInfo {
    cmds: EnsuredCmds,
    lsblk_info: LsblkInfo,
}

impl LinuxOSInfo {
    pub fn new(req_cmds: &[&str]) -> Result<LinuxOSInfo, MigError> {
        let mut cmds = EnsuredCmds::new();
        cmds.ensure_cmds(req_cmds)?;

        let lsblk_info = LsblkInfo::all(&cmds)?;
        // TODO: add cmds required by LinuxOSInfo
        Ok(LinuxOSInfo { cmds, lsblk_info })
    }

    // TODO: call command interface incl ensured commands

    pub fn get_mem_info(&self) -> Result<(u64, u64), MigError> {
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

    pub fn get_fs_space<P: AsRef<Path>>(&self, path: P) -> Result<(u64, u64), MigError> {
        const SIZE_REGEX: &str = r#"^(\d+)K?$"#;
        let path = path.as_ref();
        trace!("get_fs_space: entered with '{}'", path.display());

        let path_str = path.to_string_lossy();
        let args: Vec<&str> = vec!["--block-size=K", "--output=size,used", &path_str];

        let cmd_res = self.cmds.call(DF_CMD, &args, true)?;

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


}

impl OSInfo for LinuxOSInfo {
    fn is_admin(&self) -> Result<bool, MigError> {
        trace!("linux_os_info::is_admin: entered");
        let admin = Some(unsafe { getuid() } == 0);
        Ok(admin.unwrap())
    }

    fn get_os_arch(&self) -> Result<OSArch, MigError> {
        trace!("get_os_arch: entered");
        let cmd_res = self
            .cmds
            .call(UNAME_CMD, &["-m"], true)
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
            if let Some(os_name) = parse_file(OS_RELEASE_FILE, &Regex::new(OS_NAME_REGEX).unwrap())?
            {
                Ok(os_name[1].clone())
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "LinuxOSInfo::get_os_name: could not be located in file {}",
                        OS_RELEASE_FILE
                    ),
                ))
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "LinuxOSInfo::get_os_name: could not locate file {}",
                    OS_RELEASE_FILE
                ),
            ))
        }
    }

    // Disk specific calls
    fn path_info_from_path<P: AsRef<Path>>(&self, path: P) -> Result<dyn PathInfo, MigError> {
        let abs_path = path
            .as_ref()
            .canonicalize()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to cannoncalize  path: '{}'",
                    path.as_ref().display()
                ),
            ))?;

        let (drive, partition) = if abs_path == PathBuf::from(ROOT_PATH) {
            let (root_device, _root_fs_type) = get_kernel_root_info()?;
            self.lsblk_info.get_devinfo_from_partition(root_device)?
        } else {
            self.lsblk_info.get_path_info(&abs_path)?
        };

        let (fs_size, fs_free) = self.get_fs_space(&abs_path)?;

        Ok(LinuxPathInfo {
            abs_path: abs_path,
            drive: drive.get_path(),
            partition: partition.get_path(),
            mountpoint: if let Some(ref mountpoint) = partition.mountpoint {
                mountpoint.clone()
            } else {
                error!(
                    "Failed to retrieve mountpoint for path: '{}'",
                    path.as_ref().display()
                );
                return Err(MigError::displayed());
            },
            drive_size: if let Some(drive_size) = drive.size {
                drive_size
            } else {
                error!(
                    "Failed to retrieve drive size for path: '{}'",
                    path.as_ref().display()
                );
                return Err(MigError::displayed());
            },
            fs_type: if let Some(ref fs_type) = partition.fstype {
                fs_type.clone()
            } else {
                error!(
                    "Failed to retrieve fs_type for path: '{}'",
                    path.as_ref().display()
                );
                return Err(MigError::displayed());
            },
            fs_size,
            fs_free,
            uuid: partition.uuid,
            part_uuid: partition.partuuid,
            label: partition.label,
        })
    }

    fn path_info_from_partition<P: AsRef<Path>>(&self, partition: P) -> Result<dyn PathInfo, MigError> {
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
