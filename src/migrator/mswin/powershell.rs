const MODULE: &str = "win_test::mswin::powershell";

const POWERSHELL: &str = "powershell.exe";
const PS_CMD_PREFIX: &str = "[System.Threading.Thread]::CurrentThread.CurrentUICulture = 'en-US';";
const PS_CMD_POSTFIX: &str = " | out-string -width 200";

const PS_ARGS_FROM_STDIN: [&'static str; 3] = ["-NonInteractive", "-Command", "-"];
//pub const POWERSHELL_GET_CMDLET_PARAMS: [&'static str; 7] =
//    ["Get-Command", "-CommandType", "Cmdlet", "|" , "out-string", "-width", "200"];
const PS_CMD_IS_ADMIN: &str = 
    "[bool](([System.Security.Principal.WindowsIdentity]::GetCurrent()).groups -match \"S-1-5-32-544\")";
const PS_CMD_IS_SECURE_BOOT: &str = "Confirm-SecureBootUEFI";

const PS_ARGS_VERSION_PARAMS: [&'static str; 1] = ["$PSVersionTable.PSVersion"];

use crate::mig_error::{MigErrCtx, MigError, MigErrorKind};
use failure::{ResultExt};
use std::io::Write;

use lazy_static::lazy_static;
use log::{debug, warn};
use regex::Regex;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::process::{Command, ExitStatus, Stdio};

pub type PSVER = (u32, u32);

// Try params:
// -NonInteractive
// -NoProfile
// Try start powershell with specific version (eg.: powershell -Version 3.0) to require that version
// Find out if called as admin
// [bool](([System.Security.Principal.WindowsIdentity]::GetCurrent()).groups -match "S-1-5-32-544")

#[derive(Debug)]
struct PSRes {
    stdout: String,
    stderr: String,
    exit_status: ExitStatus,
    ps_ok: bool,
}

#[derive(Debug)]
pub(crate) struct PSInfo {
    version: Option<PSVER>,
    cmdlets: HashMap<String, bool>,
    admin: Option<bool>,
    secure_boot: Option<bool>,
}

trait PsFailed<T> {
    fn ps_failed(ps_res: &PSRes, command: &T, function: &str) -> MigError;
}

impl PSInfo {
    pub fn try_init() -> Result<PSInfo, MigError> {
        let mut ps_info = PSInfo {
            version: None,
            cmdlets: HashMap::new(),
            admin: None,
            secure_boot: None,
        };

        ps_info.get_ps_ver()?;

        debug!(
            "{}::try_init: ps_info.has_command('Get-Command')? = '{}'",
            MODULE,
            ps_info.has_command("Get-Command")?
        );

        // TODO: rather implement check commands - check if required commads are availabler,
        // ps_info.get_cmdlets()?;

        // info!("{}::try_init: result: {:?}", MODULE, ps_info);
        Ok(ps_info)
    }

    pub fn has_command(&mut self, cmd: &str) -> Result<bool, MigError> {
        match self.cmdlets.get(cmd) {
            Some(v) => return Ok(*v),
            None => (),
        }

        let cmd_res = call_from_stdin(&format!("Get-Command {};", cmd), true)?;
        if cmd_res.ps_ok {
            self.cmdlets.insert(String::from(cmd), true);
            Ok(true)
        } else {
            self.cmdlets.insert(String::from(cmd), false);
            Ok(false)
        }
    }

    pub fn is_admin(&mut self) -> Result<bool, MigError> {
        if let Some(v) = self.admin {
            Ok(v)
        } else {
            let output = call_from_stdin(PS_CMD_IS_ADMIN, true)?;
            if !output.ps_ok {
                return Err(ps_failed_stdin(&output, &PS_CMD_IS_ADMIN, "is_admin"));
            }
            self.admin = Some(output.stdout.to_lowercase() == "true");
            Ok(self.admin.unwrap())
        }
    }

    pub fn is_secure_boot(&mut self) -> Result<bool, MigError> {
        if let Some(v) = self.secure_boot {
            Ok(v)
        } else {
            if !self.is_admin()? {
                return Err(MigError::from(MigErrorKind::AuthError));
            }
            let output = call_from_stdin(&PS_CMD_IS_SECURE_BOOT, true)?;
            debug!("{}::is_secure_boot: command result: {:?}", MODULE, output);
            if !output.ps_ok || !output.stderr.is_empty() {
                // 'Confirm-SecureBootUEFI : Variable is currently undefined: 0xC0000100'
                let regex = Regex::new(
                    r"Confirm-SecureBootUEFI\s*:\s*Variable\s+is\s+currently\s+undefined:.*",
                )
                .unwrap();
                if regex.is_match(&output.stderr) {
                    self.secure_boot = Some(output.stdout.to_lowercase() == "true");
                } else {
                    return Err(ps_failed_call(
                        &output,
                        &PS_CMD_IS_SECURE_BOOT,
                        "is_secure_boot",
                    ));
                }
            } else {
                self.secure_boot = Some(output.stdout.to_lowercase() == "true");
            }
            Ok(self.secure_boot.unwrap())
        }
    }

    pub fn get_ps_ver(&mut self) -> Result<(u32, u32), MigError> {
        debug!("{}::get_ps_ver(): called", MODULE);

        match self.version {
            Some(v) => return Ok(v),
            None => (),
        }

        debug!("{}::get_ps_ver(): calling powershell", MODULE);
        let output = call(&PS_ARGS_VERSION_PARAMS, true)?;

        debug!(
            "{}::get_ps_ver(): powershell stdout: {}",
            MODULE,
            output.stdout
        );
        debug!(
            "{}::get_ps_ver(): powershell stderr {}",
            MODULE,
            output.stderr
        );

        let lines: Vec<&str> = output.stdout.lines().collect();

        debug!(
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

    
    pub fn get_drive_supported_size(&mut self, driveletter: &str) -> Result<(u64, u64), MigError> {
        if !self.is_admin()? {
            return Err(MigError::from(MigErrorKind::AuthError));
        }

        const COMMAND: &str = "Get-PartitionSupportedSize"; 
        if !self.has_command(COMMAND)? {
            return Err(MigError::from_remark(MigErrorKind::FeatureMissing, &format!("{}::get_part_supported_size: command not supported by powershell: '{}'", MODULE, COMMAND)));
        }
        
        let cmd_str = format!("{} -DriveLetter {} ",COMMAND, driveletter);
        let output = call_from_stdin(&cmd_str, true)?;

        if !output.ps_ok || !output.stderr.is_empty() {
            return Err(ps_failed_call(
                        &output,
                        &cmd_str,
                        "get_part_supported_size",
                    ));
        }

        let lines: Vec<&str> = output.stdout.lines().collect();

        debug!(
            "{}::get_part_supported_size(): powershell stdout: lines: {}",
            MODULE,
            lines.len()
        );

        /* expect
        SizeMin  SizeMax
        -------  -------
        16777216 16777216
        */

        match lines.len() {
            3 => (),
            _ => return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::get_part_supported_size: unexpected number of ouput lines in powershell version output: {}", MODULE, output.stdout)))
        }

        let headers: Vec<&str> = lines[0].split_whitespace().collect();
        let values: Vec<&str> = lines[2].split_whitespace().collect();
        let mut sizes: (u64,u64) = (0,0);

        for (idx,hdr) in headers.iter().enumerate() {
            if hdr == &"SizeMin" {
                sizes.0 = if let Some(val) = values.get(idx) {
                    val.parse::<u64>()
                        .context(MigErrCtx::from_remark(MigErrorKind::InvParam,&format!("{}::get_part_supported_size: failed to parse value to u64: '{}'", MODULE, val)))?
                } else {
                    return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::get_part_supported_size: missing value encountered in powershell version output: {}", MODULE, output.stdout)))    
                }
            } else if hdr == &"SizeMax" {
                sizes.1 = if let Some(val) = values.get(idx) {
                    val.parse::<u64>()
                        .context(MigErrCtx::from_remark(MigErrorKind::InvParam,&format!("{}::get_part_supported_size: failed to parse value to u64: '{}'", MODULE, val)))?
                } else {
                    return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::get_part_supported_size: missing value encountered in powershell version output: {}", MODULE, output.stdout)))    
                }
            } else {
                return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::get_part_supported_size: invalid header encountered in powershell version output: {}", MODULE, output.stdout)))
            }
        }

        Ok(sizes)
    }

    /*
    fn get_cmdlets(&mut self) -> Result<usize, MigError> {
        debug!("{}::get_cmdlets(): called", MODULE);
        let output = call_from_stdin(PS_CMD_GET_CMDLET_PARAMS, true)?;

        if !output.ps_ok {
            warn!("{}::get_cmdlets: powershell command failed:", MODULE);
            warn!(
                "{}::get_cmdlets:   command: '{}'",
                MODULE, PS_CMD_GET_CMDLET_PARAMS
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
                                    debug!("{}::get_cmdlets(): added cmdlet '{}'", MODULE, *v);
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
                        debug!("{}::get_cmdlets(): added cmdlet '{}'", MODULE, *v);
                        cmds += 1;
                    } else {
                        warn!("{}::get_cmdlets(): duplicate cmdlet '{}'", MODULE, *v);
                    },
                None => return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::get_cmdlets: name value not found in output from: powershell Get-Commands",MODULE))),
            };
        }
        Ok(cmds)
    }
    */
}

fn call_from_stdin(cmd_str: &str, trim_stdout: bool) -> Result<PSRes, MigError> {
    debug!(
        "{}::call_from_stdin(): called with {:?} < '{}'  trim_stdout: {}",
        MODULE,
        PS_ARGS_FROM_STDIN,
        cmd_str,
        trim_stdout
    );
    let mut command = Command::new(POWERSHELL)
        .args(&PS_ARGS_FROM_STDIN)
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

    let mut full_cmd = String::from(PS_CMD_PREFIX);
    full_cmd.push_str(cmd_str);
    full_cmd.push_str(PS_CMD_POSTFIX);
    if let Some(ref mut stdin) = command.stdin {
        stdin
            .write(full_cmd.as_bytes())
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
    debug!(
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
    warn!(
        "{}::{}: powershell command failed: '{:?}'",
        MODULE, function, command
    );
    warn!(
        "{}::{}:   exit code: {}",
        MODULE,
        function,
        ps_res.exit_status.code().unwrap_or(0)
    );
    warn!("{}::{}:   stderr: '{}'", MODULE, function, &ps_res.stderr);
    MigError::from(MigErrorKind::PSFailed)
}

fn ps_failed_stdin<T: Display>(ps_res: &PSRes, command: &T, function: &str) -> MigError {
    warn!(
        "{}::{}: powershell command failed: '{}'",
        MODULE, function, command
    );
    warn!(
        "{}::{}:   exit code: {}",
        MODULE,
        function,
        ps_res.exit_status.code().unwrap_or(0)
    );
    warn!("{}::{}:   stderr: '{}'", MODULE, function, &ps_res.stderr);
    MigError::from(MigErrorKind::PSFailed)
}
