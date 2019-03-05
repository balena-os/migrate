use  crate::common::mig_error::{MigError,MigErrorCode};
use  crate::common::SysInfo;

use std::error::Error;
use std::io::prelude::*;
use std::process::{Command, Stdio};
use std::vec::Vec;

const OS: &str = "windows";

pub fn available() -> bool {    
    return cfg!(windows);
}

pub fn sys_info() -> Result<SysInfo,MigError> {
    if cfg!(windows) {
    // Spawn the command `powershell Systeminfo /FO CSV`
    let process = match Command::new("powershell.exe")
                                .args(&["Systeminfo", "/FO", "CSV"])
                                .stdout(Stdio::piped())
                                .spawn() {
        Err(why) => { return Err(MigError::from_code(MigErrorCode::ErrExecProcess, "failed to execute: powershell Systeminfo /FO CSV", Some(Box::new(why)))) },
        Ok(process) => process,
    };

    // The `stdout` field also has type `Option<ChildStdout>` so must be unwrapped.

    let mut out_str : String = String::from("");
    let lines = match process.stdout {
        // TODO: how to return why as source source
        None => { return Err(MigError::from_code(MigErrorCode::ErrCmdIO, "failed to read command output from: powershell Systeminfo /FO CSV", None )) }
        Some(mut std_out) => {            
            match std_out.read_to_string(&mut out_str) {
                Ok(r) => { 
                    println!("powershell Systeminfo /FO CSV:\n{}", out_str);
                    let lines: Vec<&str> = out_str.lines().collect();
                    lines 
                }, 
                Err(why) => { return Err(MigError::from_code(MigErrorCode::ErrCmdIO, "failed to read command output from: powershell Systeminfo /FO CSV", Some(Box::new(why)))) },
                }
            }
        };

    
    match lines.len() {
        2 => (),
        _ => return Err(MigError::from_code(MigErrorCode::ErrInvParam, "unexpected number of output lines received from: powershell Systeminfo /FO CSV", None)) 
    }

    let headers = lines[0];
    let data = lines[1];

    Ok(SysInfo::new("windows"))
    } else {
        Err(MigError::from_code(MigErrorCode::ErrInvOSType, "invalid OS, not windows",None))
    }
}

pub fn process() -> Result<(),MigError> {
    let s_info = sys_info()?;
    println!("sysInfo: {:?}",s_info);
    Ok(())
}