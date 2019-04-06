use failure::{Context, ResultExt};
use libc::{getuid, sysinfo};
use log::{error, info, debug};

// use std::os::linux::{};
use lazy_static::lazy_static;
use regex::Regex;

mod util;

use crate::migrator::{
    common::{call, CmdRes},
    MigErrCtx, 
    MigError, 
    MigErrorKind,
    Migrator, 
    OSArch, 
    OSRelease};

use std::collections::hash_map::HashMap;

const MODULE: &str = "Linux";
const OS_NAME_RE: &str = r#"^PRETTY_NAME="([^"]+)"$"#;

const OS_RELEASE_FILE: &str = "/etc/os-release";
const OS_KERNEL_RELEASE_FILE: &str = "/proc/sys/kernel/osrelease";
const OS_MEMINFO_FILE: &str = "/proc/meminfo";
const SYS_UEFI_DIR: &str = "/sys/firmware/efi";
const UNAME_CMD: &str = "uname";
const UNAME_ARGS_OS_ARCH: [&str; 1] = ["-m"];

const FINDMNT_CMD: &str = "findmnt";
const FINDMNT_ARGS_BOOT: [&str; 5] = [
    "--noheadings",
    "--canonicalize",
    "--output",
    "SOURCE",
    "/boot",
];
const FINDMNT_ARGS_ROOT: [&str; 5] = ["--noheadings", "--canonicalize", "--output", "SOURCE", "/"];

const MOKUTIL_CMD: &str = "mokutil";
const MOKUTIL_ARGS_SB_STATE: [&str; 1] = ["--sb-state"];

pub(crate) struct LinuxMigrator {
    os_name: Option<String>,
    os_release: Option<OSRelease>,
    os_arch: Option<OSArch>,
    uefi_boot: Option<bool>,
    boot_dev: Option<String>,
    cmd_path: HashMap<String, String>,
    mem_tot: Option<u64>,
    mem_free: Option<u64>,
    admin: Option<bool>,
    sec_boot: Option<bool>,
}

impl LinuxMigrator {
    pub fn try_init() -> Result<LinuxMigrator, MigError> {
        Ok(LinuxMigrator {
            os_name: None,
            os_release: None,
            os_arch: None,
            uefi_boot: None,
            boot_dev: None,
            cmd_path: HashMap::new(),
            mem_tot: None,
            mem_free: None,
            admin: None,
            sec_boot: None,
        })
    }
}

impl LinuxMigrator {
    fn call_cmd(
        &mut self,
        cmd: &str,
        args: &[&str],
        trim_stdout: bool,
    ) -> Result<CmdRes, MigError> {
        Ok(call(
            self.cmd_path
                .entry(String::from(cmd))
                .or_insert(util::whereis(cmd)?),
            args,
            trim_stdout,
        )?)
    }

    fn get_mem_info(&mut self) -> Result<(), MigError> {
        use std::mem;
        let mut s_info: libc::sysinfo = unsafe { mem::uninitialized() };
        let res = unsafe { libc::sysinfo(&mut s_info) };
        if res == 0 {
            self.mem_tot = Some(s_info.totalram as u64);
            self.mem_free = Some(s_info.freeram as u64);
            Ok(())
        } else {
            Err(MigError::from(MigErrorKind::NotImpl))
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

impl Migrator for LinuxMigrator {
    fn get_os_name<'a>(&'a mut self) -> Result<&'a str, MigError> {
        // TODO: ensure availabilty of method
        match self.os_name {
            Some(ref s) => Ok(s),
            None => {
                lazy_static! {
                    static ref RE: Regex = Regex::new(OS_NAME_RE).unwrap();
                    // static ref RE: Regex = Regex::new("^PRETTY_NAME=\"([^\"]+)$").unwrap();
                }

                self.os_name = Some(util::parse_file("/etc/os-release", &RE)?);
                Ok(self.os_name.as_ref().unwrap())
            }
        }
    }

    fn get_os_release<'a>(&'a mut self) -> Result<&'a OSRelease, MigError> {
        match self.os_release {
            Some(ref s) => Ok(s),
            None => {
                let os_info = std::fs::read_to_string(OS_KERNEL_RELEASE_FILE).context(
                    MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("File read '{}'", OS_KERNEL_RELEASE_FILE),
                    ),
                )?;

                self.os_release = Some(OSRelease::parse_from_str(&os_info.trim())?);

                Ok(self.os_release.as_ref().unwrap())
            }
        }
    }

    fn is_uefi_boot(&mut self) -> Result<bool, MigError> {
        match self.uefi_boot {
            Some(u) => Ok(u),
            None => {
                match std::fs::metadata(SYS_UEFI_DIR) {
                    Ok(metadata) => {
                        self.uefi_boot = Some(metadata.file_type().is_dir());
                    }
                    Err(why) => {
                        match why.kind() {
                            std::io::ErrorKind::NotFound => {
                                self.uefi_boot = Some(false);
                            }
                            // TODO: figure out how to create a MigError with context manually
                            _ => {
                                return Err(MigError::from_remark(
                                    MigErrorKind::Upstream,
                                    &format!("{}::is_uefi_boot: access {}", MODULE, SYS_UEFI_DIR),
                                ));
                            }
                            //_ => { return Err(MigError::from(why.context(MigErrCtx::from_remark(MigErrorKind::Upstream,&format!("{}::is_uefi_boot: access {}",MODULE,SYS_UEFI_DIR))))); },
                        }
                    }
                }
                Ok(self.uefi_boot.unwrap())
            }
        }
    }

    fn get_os_arch<'a>(&'a mut self) -> Result<&'a OSArch, MigError> {
        match self.os_arch {
            Some(ref u) => Ok(u),
            None => {
                let cmd_res = self
                    .call_cmd(UNAME_CMD, &UNAME_ARGS_OS_ARCH, true)
                    .context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("{}::get_os_arch: call {}", MODULE, UNAME_CMD),
                    ))?;
                if cmd_res.status.success() {
                    if cmd_res.stdout.to_lowercase() == "x86_64" {
                        self.os_arch = Some(OSArch::AMD64);
                    } else if cmd_res.stdout.to_lowercase() == "i386" {
                        self.os_arch = Some(OSArch::I386);
                    } else if cmd_res.stdout.to_lowercase() == "armv7l" {
                        self.os_arch = Some(OSArch::ARMHF);
                    } else {
                        return Err(MigError::from_remark(
                            MigErrorKind::InvParam,
                            &format!(
                                "{}::get_os_arch: unknown architectute '{}'",
                                MODULE, cmd_res.stdout
                            ),
                        ));
                    }
                    Ok(self.os_arch.as_ref().unwrap())
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
        }
    }

    fn get_boot_dev<'a>(&'a mut self) -> Result<&'a str, MigError> {
        match self.boot_dev {
            Some(ref u) => Ok(u),
            None => {
                let mut cmd_res = self
                    .call_cmd(FINDMNT_CMD, &FINDMNT_ARGS_BOOT, true)
                    .context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("{}::get_os_arch: call {}", MODULE, FINDMNT_CMD),
                    ))?;
                if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
                    cmd_res = self
                        .call_cmd(FINDMNT_CMD, &FINDMNT_ARGS_ROOT, true)
                        .context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!("{}::get_os_arch: call {}", MODULE, FINDMNT_CMD),
                        ))?;
                    if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
                        return Err(MigError::from_remark(
                            MigErrorKind::ExecProcess,
                            &format!(
                                "{}::get_os_arch: command failed: {}",
                                MODULE,
                                cmd_res.status.code().unwrap_or(0)
                            ),
                        ));
                    }
                }
                self.boot_dev = Some(cmd_res.stdout);
                Ok(self.boot_dev.as_ref().unwrap())
            }
        }
    }

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

    fn is_admin(&mut self) -> Result<bool, MigError> {
        match self.admin {
            Some(v) => Ok(v),
            None => {
                self.admin = Some(unsafe { getuid() } == 0);
                Ok(self.admin.unwrap())
            }
        }
    }

    fn is_secure_boot(&mut self) -> Result<bool, MigError> {
        match self.sec_boot {
            Some(v) => Ok(v),
            None => {
                let cmd_res = match self.call_cmd(MOKUTIL_CMD, &MOKUTIL_ARGS_SB_STATE, true) {
                    Ok(cr) => {
                        debug!("{}::is_secure_boot: {} -> {:?}", MODULE, MOKUTIL_CMD, cr);
                        cr
                    }
                    Err(why) => {
                        debug!("{}::is_secure_boot: {} -> {:?}", MODULE, MOKUTIL_CMD, why);
                        match why.kind() {
                            MigErrorKind::NotFound => {
                                self.sec_boot = Some(false);
                                return Ok(self.sec_boot.unwrap());
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
                            self.sec_boot = Some(true);
                        } else {
                            self.sec_boot = Some(false);
                        }
                        return Ok(self.sec_boot.unwrap());
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
        }
    }

    fn can_migrate(&mut self) -> Result<bool, MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn migrate(&mut self) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
}
