use  crate::common::mig_error::{MigError,MigErrorCode};
use  crate::common::SysInfo;

use log::{trace,error};

use std::io::prelude::*;
use std::process::{Command, Stdio};
use std::vec::Vec;

const OS: &str = "windows";

pub fn available() -> bool {    
    trace!("called available()");
    return cfg!(windows);
}

pub fn sys_info() -> Result<SysInfo,MigError> {
    trace!("called sys_info()");
    if cfg!(windows) {
    // Spawn the command `powershell Systeminfo /FO CSV`
    let process = match Command::new("powershell.exe")
                                .args(&["Systeminfo", "/FO", "CSV"])
                                .stdout(Stdio::piped())
                                .stderr(Stdio::piped())
                                .spawn() {
        Err(why) => { 
            // TODO: extract stderr & add to returned error
            return Err(MigError::from_code(MigErrorCode::ErrExecProcess, "failed to execute: powershell Systeminfo /FO CSV", Some(Box::new(why)))) },
        Ok(process) => process,
    };

    // The `stdout` field also has type `Option<ChildStdout>` so must be unwrapped.

    let mut out_str : String = String::from("");
    let lines = match process.stdout {
        // TODO: how to return why as source source
        None => { return Err(MigError::from_code(MigErrorCode::ErrCmdIO, "failed to read command output from: powershell Systeminfo /FO CSV", None )) }
        Some(mut std_out) => {            
            match std_out.read_to_string(&mut out_str) {
                Ok(_r) => { 
                    let lines: Vec<&str> = out_str.lines().collect();
                    lines 
                }, 
                Err(why) => { return Err(MigError::from_code(MigErrorCode::ErrCmdIO, "failed to read command output from: powershell Systeminfo /FO CSV", Some(Box::new(why)))) },
                }
            }
        };

    
    match lines.len() {
        2 => (),            
        _ => { 
            error!("sys_info: too many lines in command output of  powershell Systeminfo /FO CSV");
            for line in lines {
                error!("sys_info: {}", line);
            }
            return Err(MigError::from_code(MigErrorCode::ErrInvParam, "unexpected number of output lines received from: powershell Systeminfo /FO CSV", None)); 
        }
    }

    let headers = lines[0];
    let data = lines[1];
    trace!("sys_info: headers: {}",headers);
    trace!("sys_info: data:    {}",data);

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