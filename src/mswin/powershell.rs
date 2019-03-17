const MODULE: &str = "win_test::mswin::powershell";

const POWERSHELL: &str = "powershell.exe";

pub const POWERSHELL_GET_CMDLET_PARAMS: [&'static str; 7] =
    ["Get-Command", "-CommandType", "Cmdlet", "|" , "out-string", "-width", "1024"];
pub const POWERSHELL_SYSINFO_PARAMS: [&'static str; 3] = ["Systeminfo", "/FO", "CSV"];
pub const POWERSHELL_VERSION_PARAMS: [&'static str; 1] = ["$PSVersionTable.PSVersion"];

use failure::{Fail,ResultExt};
use crate::mig_error::{MigError, MigErrorKind, MigErrCtx};

use lazy_static::lazy_static;
use regex::Regex;
use log::{trace, warn};
use std::collections::HashSet;
use std::process::{Command, Stdio};


pub type PSVER = (u32,u32);

// Find out if called as admin
// [bool](([System.Security.Principal.WindowsIdentity]::GetCurrent()).groups -match "S-1-5-32-544")

struct PSRes {
    stdout: String,
    stderr: String,
} 

pub(crate) struct PSInfo {
    ps_ver: Option<PSVER>,
    ps_cmdlets: HashSet<String>,
}

impl PSInfo {
    pub fn try_init() -> Result<PSInfo, MigError> {
        let mut ps_info = PSInfo {
            ps_ver: None,
            ps_cmdlets: HashSet::new(),
        };

        match ps_info.get_ps_ver() {
            Ok(_v) => (),
            Err(why) => return Err(why),
        };

        match ps_info.get_cmdlets() {
            Ok(_v) => (),
            Err(why) => return Err(why),
        };

        Ok(ps_info)
    }

    pub fn has_command(&mut self, cmd: &str) -> bool {
        self.ps_cmdlets.contains(cmd)        
    }

    pub fn get_ps_ver(&mut self) -> Result<(u32, u32), MigError> {
        trace!("{}::get_ps_ver(): called", MODULE);

        match self.ps_ver {
            Some(v) => return Ok(v),
            None => (),
        }

        trace!("{}::get_ps_ver(): calling powershell", MODULE);
        let output = call_to_string(&POWERSHELL_VERSION_PARAMS, true)?;

        trace!(
            "{}::get_ps_ver(): powershell stdout: {}",
            MODULE,
            output.stdout
        );
        trace!(
            "{}::get_ps_ver(): powershell stderr {}",
            MODULE,
            output.stderr
        );

        let lines: Vec<&str> = output.stdout.lines().collect();

        trace!(
            "{}::get_ps_ver(): powershell stdout: lines: {}",
            MODULE,
            lines.len()
        );

        match lines.len() {
            3 => (),
            0 => {
                warn!("{}::get_ps_ver(): no output from command, assuming version 1.0", MODULE);
                self.ps_ver = Some((1, 0));
                return Ok(self.ps_ver.unwrap())
            }        
            _ => return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::available(): unexpected number of ouput lines in powershell version output: {}", MODULE, output.stdout)))
        }

        let headers: Vec<&str> = lines[0].split_whitespace().collect();
        let values: Vec<&str> = lines[2].split_whitespace().collect();

        let mut major: u32 = 1;
        let mut minor: u32 = 0;

        for idx in 0..headers.len() {
            let hdr: &str = &headers[idx];
            match hdr {
                "Major" => {
                    major = values.get(idx).unwrap().parse().unwrap();
                }
                "Minor" => {
                    minor = values.get(idx).unwrap().parse().unwrap();
                }
                _ => {
                    break;
                }
            }
        }

        self.ps_ver = Some((major, minor));
        Ok(self.ps_ver.unwrap())
    }

    fn get_cmdlets(&mut self) -> Result<usize, MigError> {
        trace!("{}::get_cmdlets(): called", MODULE);
        let output = call_to_string(&POWERSHELL_GET_CMDLET_PARAMS, true)?;

        lazy_static! {
            static ref RE: Regex = Regex::new(r"^-+$").unwrap();
        }

        let mut lines = output.stdout.lines().enumerate();

        // find 'Name' in headers ans save word index  
        let mut name_idx: Option<usize> = None;
        let mut cmds: usize = 0;

        for header in match lines.next() {
            Some(s) => s.1.split_whitespace().enumerate(),
            None => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::get_cmdlets: 0 output lines received from: powershell Get-Commands",
                        MODULE
                    )));
            }
        } {
            if header.1 == "Name" {
                name_idx = Some(header.0);
                break;
            }
        }

        let name_idx = match name_idx {
            Some(n) => n,
            None => return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::get_cmdlets: name header not found in output from: powershell Get-Commands",MODULE))),
        };

        // potentitally skip line with ----
        match lines.next() {
            Some(s) => {
                let words: Vec<&str> = s.1.split_whitespace().collect();
                match words.get(name_idx) {
                        Some(v) => {
                            if !RE.is_match(v) {
                                if self.ps_cmdlets.insert(String::from(*v)) {
                                    cmds += 1;
                                    trace!("{}::get_cmdlets(): added cmdlet '{}'", MODULE, *v);
                                } else {
                                    warn!("{}::get_cmdlets(): duplicate cmdlet '{}'", MODULE, *v);
                                }
                            }
                        },
                        None => return Err(MigError::from_remark(
                            MigErrorKind::InvParam, 
                            &format!("{}::get_cmdlets: name value not found in output from: powershell Get-Commands",MODULE))),
                    }
            }
            None => return Ok(0),
        }

        for line in lines {
            let words: Vec<&str> = line.1.split_whitespace().collect();
            match words.get(name_idx) {
                Some(v) => 
                    if self.ps_cmdlets.insert(String::from(*v)) {
                        trace!("{}::get_cmdlets(): added cmdlet '{}'", MODULE, *v);
                        cmds += 1;
                    } else {
                        warn!("{}::get_cmdlets(): duplicate cmdlet '{}'", MODULE, *v);
                    },                    
                None => return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::get_cmdlets: name value not found in output from: powershell Get-Commands",MODULE))),
            };
        }
        Ok(cmds)
    }
}

fn call_to_string(args: &[&str], trim_stdout: bool) -> Result<PSRes, MigError> {
    trace!("{}::call_to_string(): called with {:?}, {}", MODULE, args, trim_stdout);
    let output = Command::new(POWERSHELL)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output().context(MigErrCtx::from_remark(MigErrorKind::Upstream,
                    &format!(
                        "{}::call_to_string: failed to execute: powershell command '{:?}'",
                        MODULE,args
                    )))?;

    if !output.status.success() {                
        return Err(MigError::from_remark(
            MigErrorKind::ExecProcess, 
            &format!("{}::init_sys_info: command failed with exit code {}", MODULE, output.status.code().unwrap_or(0))));
    }

    // TODO: use os str instead
    let stdout_str = String::from_utf8(output.stdout).context(
        MigErrCtx::from_remark(MigErrorKind::InvParam,&format!("{}::call_to_string: invalid utf8 in stdout", MODULE)))?;

    let stderr_str = String::from_utf8(output.stderr).context(
        MigErrCtx::from_remark(
            MigErrorKind::InvParam,
            &format!("{}::call_to_string: invalid utf8 in stderr", MODULE)))?;

    Ok(PSRes {
        stdout: match trim_stdout {
            true => String::from(stdout_str.trim()),
            false => stdout_str,
        },
        stderr: stderr_str,
    })
}


