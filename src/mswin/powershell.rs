const MODULE: &str = "win_test::mswin::powershell";

const POWERSHELL: &str = "powershell.exe";

pub const POWERSHELL_FROM_STDIN: [&'static str; 3] = ["-NonInteractive", "-Command", "-"];
//pub const POWERSHELL_GET_CMDLET_PARAMS: [&'static str; 7] =
//    ["Get-Command", "-CommandType", "Cmdlet", "|" , "out-string", "-width", "200"];
pub const PSCMD_STR_GET_CMDLET_PARAMS: &str =
    "Get-Command -CommandType Cmdlet | Format-Table Name, Version | out-string -width 200";
pub const PSCMD_STR_IS_ADMIN: &str =
    "[bool](([System.Security.Principal.WindowsIdentity]::GetCurrent()).groups -match \"S-1-5-32-544\")";

pub const POWERSHELL_VERSION_PARAMS: [&'static str; 1] = ["$PSVersionTable.PSVersion"];
pub const POWERSHELL_IS_SECURE_BOOT: [&'static str; 1] = ["Confirm-SecureBootUEFI"];

use crate::mig_error::{MigErrCtx, MigError, MigErrorKind};
use failure::{Fail, ResultExt};
use std::io::{Write};

use lazy_static::lazy_static;
use log::{trace, info, warn};
use regex::Regex;
use std::collections::HashSet;
use std::process::{Command, ExitStatus, Stdio};
use std::fmt::{Display, Debug};

pub type PSVER = (u32, u32);

// Try params:
// -NonInteractive
// -NoProfile
// Try start powershell with specific version (eg.: powershell -Version 3.0) to require that version
// Find out if called as admin
// [bool](([System.Security.Principal.WindowsIdentity]::GetCurrent()).groups -match "S-1-5-32-544")

struct PSRes {
    stdout: String,
    stderr: String,
    exit_status: ExitStatus,
    ps_ok: bool,
}

#[derive(Debug)]
pub(crate) struct PSInfo {
    version: Option<PSVER>,
    cmdlets: HashSet<String>,
    is_admin: Option<bool>,
}

trait PsFailed<T> {
   fn ps_failed(ps_res: &PSRes, command: &T, function: &str) -> MigError;   
}

impl PSInfo {
    pub fn try_init() -> Result<PSInfo, MigError> {

        let mut ps_info = PSInfo {
            version: None,
            cmdlets: HashSet::new(),
            is_admin: None,
        };
        
        ps_info.get_ps_ver()?;

        // TODO: rather implement check commands - check if required commads are availabler, 
        ps_info.get_cmdlets()?;

        // info!("{}::try_init: result: {:?}", MODULE, ps_info);
        Ok(ps_info)
    }

    pub fn has_command(&self, cmd: &str) -> bool {
        self.cmdlets.contains(cmd)
    }

    pub fn is_admin(&mut self) -> Result<bool,MigError> {
        if let Some(v) = self.is_admin {
            Ok(v)
        } else {
            let output = call_from_stdin(PSCMD_STR_IS_ADMIN, true)?;
            if !output.ps_ok {
                return Err(ps_failed_stdin(&output,&PSCMD_STR_IS_ADMIN, "is_admin"));
            }
            let val = output.stdout.to_lowercase() == "true";
            self.is_admin = Some(val);
            Ok(val)
        }
    }

    pub fn is_secure_boot(&mut self) -> Result<bool,MigError> {
        if ! self.is_admin()? {
            return Err(MigError::from(MigErrorKind::AuthError));
        }
        let output = call(&POWERSHELL_IS_SECURE_BOOT,true)?;
        if !output.ps_ok || !output.stderr.is_empty() {            
            return Err(ps_failed_call(&output, &POWERSHELL_IS_SECURE_BOOT, "is_secure_boot"));
        }

        Ok(output.stdout.to_lowercase() == "true")
    }

    

    pub fn get_ps_ver(&mut self) -> Result<(u32, u32), MigError> {
        trace!("{}::get_ps_ver(): called", MODULE);

        match self.version {
            Some(v) => return Ok(v),
            None => (),
        }

        trace!("{}::get_ps_ver(): calling powershell", MODULE);
        let output = call(&POWERSHELL_VERSION_PARAMS, true)?;

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
                self.version = Some((1, 0));
                return Ok(self.version.unwrap())
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

        self.version = Some((major, minor));
        Ok(self.version.unwrap())
    }

    fn get_cmdlets(&mut self) -> Result<usize, MigError> {
        trace!("{}::get_cmdlets(): called", MODULE);
        let output = call_from_stdin(PSCMD_STR_GET_CMDLET_PARAMS, true)?;

        if !output.ps_ok {
            warn!("{}::get_cmdlets: powershell command failed:", MODULE);
            warn!(
                "{}::get_cmdlets:   command: '{}'",
                MODULE, PSCMD_STR_GET_CMDLET_PARAMS
            );
            warn!(
                "{}::get_cmdlets:   exit code: {}",
                MODULE,
                output.exit_status.code().unwrap_or(0)
            );
            warn!("{}::get_cmdlets:   stderr: '{}'", MODULE, &output.stderr);
            return Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &format!(
                    "{}::get_cmdlets: command returned non zero exit status: {}",
                    MODULE,
                    output.exit_status.code().unwrap_or(0)
                ),
            ));
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
                    ),
                ));
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
                lazy_static! {
                    static ref RE: Regex = Regex::new(r"^-+$").unwrap();
                }
                let words: Vec<&str> = s.1.split_whitespace().collect();
                match words.get(name_idx) {
                        Some(v) => {
                            if !RE.is_match(v) {
                                if self.cmdlets.insert(String::from(*v)) {
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
                    if self.cmdlets.insert(String::from(*v)) {
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

fn call_from_stdin(cmd_str: &str, trim_stdout: bool) -> Result<PSRes, MigError> {
    trace!(
        "{}::call_from_stdin(): called with {:?} < '{}'  trim_stdout: {}",
        MODULE,
        POWERSHELL_FROM_STDIN,
        cmd_str,
        trim_stdout
    );
    let mut command = Command::new(POWERSHELL)
        .args(&POWERSHELL_FROM_STDIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "{}::call_from_stdout: failed to execute: powershell command '{:?}'",
                MODULE, cmd_str
            ),
        ))?;
    // TODO: make sure we write the right thing (utf8/wide)
    if let Some(ref mut stdin) = command.stdin {
        stdin
            .write(cmd_str.as_bytes())
            .context(MigErrCtx::from_remark(
                MigErrorKind::CmdIO,
                &format!("{}::call_from_stdin: failed to write to stdin", MODULE),
            ))?;
    } else {
        panic!("{}::call_from_stdin: no stdin found for process", MODULE);
    }

    let output = command
        .wait_with_output()
        .context(MigErrCtx::from(MigErrorKind::ExecProcess))?;

    let mut ps_ok = output.status.success();
    let stderr = String::from(String::from_utf8_lossy(&output.stderr));    

    if ps_ok && !stderr.is_empty() {
        lazy_static! {
            static ref RE: Regex = Regex::new(r"^At line:\d+ char:\d+").unwrap();
        }
        ps_ok = !RE.is_match(&stderr);
    } 

    Ok(PSRes {
        stdout: match trim_stdout {
            true => String::from(String::from_utf8_lossy(&output.stdout).trim()),
            false => String::from(String::from_utf8_lossy(&output.stdout)),
        },
        stderr: String::from(String::from_utf8_lossy(&output.stderr)),
        exit_status: output.status,
        ps_ok,
    })
}

fn call(args: &[&str], trim_stdout: bool) -> Result<PSRes, MigError> {
    trace!(
        "{}::call_to_string(): called with {:?}, {}",
        MODULE,
        args,
        trim_stdout
    );

    let output = Command::new(POWERSHELL)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "{}::call_to_string: failed to execute: powershell command '{:?}'",
                MODULE, args
            ),
        ))?;

    let mut ps_ok = output.status.success();
    let stderr = String::from(String::from_utf8_lossy(&output.stderr));    

    if ps_ok && !stderr.is_empty() {
        lazy_static! {
            static ref RE: Regex = Regex::new(r"^At line:\d+ char:\d+").unwrap();
        }
        ps_ok = !RE.is_match(&stderr);
    } 

    Ok(PSRes {
        stdout: match trim_stdout {
            true => String::from(String::from_utf8_lossy(&output.stdout).trim()),
            false => String::from(String::from_utf8_lossy(&output.stdout)),
        },
        stderr: stderr,
        exit_status: output.status,
        ps_ok,
    })
}

fn ps_failed_call<T: Debug>(ps_res: &PSRes, command: &T, function: &str) -> MigError {    
    warn!("{}::{}: powershell command failed: '{:?}'", MODULE, function, command);    
    warn!("{}::{}:   exit code: {}", MODULE, function, ps_res.exit_status.code().unwrap_or(0));
    warn!("{}::{}:   stderr: '{}'", MODULE, function, &ps_res.stderr);
    MigError::from(MigErrorKind::PSFailed)
}

fn ps_failed_stdin<T: Display>(ps_res: &PSRes, command: &T, function: &str) -> MigError {    
    warn!("{}::{}: powershell command failed: '{}'", MODULE, function, command);    
    warn!("{}::{}:   exit code: {}", MODULE, function, ps_res.exit_status.code().unwrap_or(0));
    warn!("{}::{}:   stderr: '{}'", MODULE, function, &ps_res.stderr);
    MigError::from(MigErrorKind::PSFailed)
}