use log::{info, trace, error};
use failure::{ResultExt};


// use std::os::linux::{};
use lazy_static::lazy_static;
use regex::Regex;

mod util;
use std::collections::hash_map::{HashMap};
use crate::mig_error::{MigError,MigErrorKind,MigErrCtx};
use crate::{OSRelease, OSArch, Migrator};
use crate::common::{call,CmdRes};

const MODULE: &str = "Linux";
const OS_NAME_RE: &str = r#"^PRETTY_NAME="([^"]+)"$"#;

const OS_RELEASE_FILE: &str = "/etc/os-release";
const OS_KERNEL_RELEASE_FILE: &str = "/proc/sys/kernel/osrelease";
const SYS_UEFI_DIR: &str = "/sys/firmware/efi";
const UNAME_CMD: &str = "uname";
const UNAME_ARGS_OS_ARCH: [&str;1] = ["-m"];

const FINDMNT_CMD: &str = "findmnt";
const FINDMNT_ARGS_BOOT: [&str;5] = ["--noheadings","--canonicalize","--output","SOURCE","/boot"];
const FINDMNT_ARGS_ROOT: [&str;5] = ["--noheadings","--canonicalize","--output","SOURCE","/"];

pub(crate) struct LinuxMigrator {
    os_name: Option<String>,
    os_release: Option<OSRelease>,
    os_arch: Option<OSArch>,
    uefi_boot: Option<bool>,
    boot_dev: Option<String>,
    cmd_path: HashMap<String,String>, 

}

impl LinuxMigrator {
    pub fn try_init() -> Result<LinuxMigrator,MigError> {
        Ok(LinuxMigrator{
            os_name: None,
            os_release: None,
            os_arch: None,
            uefi_boot: None,
            boot_dev: None,
            cmd_path: HashMap::new(),
        })
    } 
}

impl LinuxMigrator {
    fn call_cmd(&mut self, cmd: &str, args: &[&str], trim_stdout: bool) -> Result<CmdRes, MigError> {
        Ok(call(
            self.cmd_path.entry(String::from(cmd)).or_insert(util::whereis(cmd)?), 
            args, 
            trim_stdout)?)
    }
}

impl Migrator for LinuxMigrator {
    fn get_os_name<'a>(&'a mut self) -> Result<&'a str,MigError> {
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

    fn get_os_release<'a>(&'a mut self) -> Result<&'a OSRelease,MigError> {
        match self.os_release {
            Some(ref s) => Ok(s),
            None => {
                let os_info = std::fs::read_to_string(OS_KERNEL_RELEASE_FILE)
                    .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("File read '{}'",OS_KERNEL_RELEASE_FILE)))?; 
                
                self.os_release = Some(OSRelease::parse_from_str(&os_info.trim())?);

                Ok(self.os_release.as_ref().unwrap())
            }
        }
    }

    fn is_uefi_boot(&mut self) -> Result<bool,MigError> {
        match self.uefi_boot {
            Some(u) => Ok(u),
            None => {
                self.uefi_boot = Some(std::fs::metadata(SYS_UEFI_DIR)
                                    .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::is_uefi_boot: access {}", MODULE, SYS_UEFI_DIR)))?
                                    .file_type()
                                    .is_dir());
                Ok(self.uefi_boot.unwrap())
            }
        }
    }


    fn get_os_arch<'a>(&'a mut self) -> Result<&'a OSArch, MigError> {        
        match self.os_arch {
            Some(ref u) => Ok(u),
            None => {
                let cmd_res = self.call_cmd(UNAME_CMD, &UNAME_ARGS_OS_ARCH,true)
                    .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::get_os_arch: call {}", MODULE, UNAME_CMD)))?;        
                if cmd_res.status.success() {
                    if cmd_res.stdout.to_lowercase() == "x86_64" {
                        self.os_arch = Some(OSArch::AMD64);
                    } else if cmd_res.stdout.to_lowercase() == "i386" {
                        self.os_arch = Some(OSArch::I386);
                    } else if cmd_res.stdout.to_lowercase() ==  "armv7l" {
                        self.os_arch = Some(OSArch::ARMHF);
                    } else {
                        return Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::get_os_arch: unknown architectute '{}'", MODULE, cmd_res.stdout)));
                    }
                    Ok(self.os_arch.as_ref().unwrap())
                } else {
                    Err(MigError::from_remark(MigErrorKind::ExecProcess,&format!("{}::get_os_arch: command failed: {}", MODULE, cmd_res.status.code().unwrap_or(0))))
                } 
            }
        }       
    }

    fn get_boot_dev<'a>(&'a mut self) -> Result<&'a str,MigError> {
        match self.boot_dev {
            Some(ref u) => Ok(u),
            None => {
                let mut cmd_res = self.call_cmd(FINDMNT_CMD, &FINDMNT_ARGS_BOOT, true)
                    .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::get_os_arch: call {}", MODULE, FINDMNT_CMD)))?;        
                if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
                    cmd_res = self.call_cmd(FINDMNT_CMD, &FINDMNT_ARGS_ROOT, true)
                        .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::get_os_arch: call {}", MODULE, FINDMNT_CMD)))?;        
                    if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
                        return Err(MigError::from_remark(MigErrorKind::ExecProcess,&format!("{}::get_os_arch: command failed: {}", MODULE, cmd_res.status.code().unwrap_or(0))));
                    }
                }
                self.boot_dev = Some(cmd_res.stdout);
                Ok(self.boot_dev.as_ref().unwrap())
            }
        }       
    }

    fn get_mem_tot(&mut self) -> Result<usize,MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn get_mem_avail(&mut self) -> Result<usize,MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn is_admin(&mut self) -> Result<bool,MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn is_secure_boot(&mut self) -> Result<bool,MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn can_migrate(&mut self) -> Result<bool,MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
    fn migrate(&mut self) -> Result<(),MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }  
}