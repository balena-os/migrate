use  crate::common::mig_error::{MigError,MigErrorCode};
use  crate::common::SysInfo;


use std::io::prelude::*;
use std::process::{Command, Stdio};

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
    let mut s = String::new();
    match process.stdout.unwrap().read_to_string(&mut s) {
        // TODO: how to return why as source source
        Err(why) => { return Err(MigError::from_code(MigErrorCode::ErrCmdIO, "failed to read command output from: powershell Systeminfo /FO CSV", Some(Box::new(why)))) },
        Ok(_) => (),
    }

    // TODO: parse command output
    
    println!("powershell Systeminfo /FO CSV:\n{}", s);

    Ok(SysInfo::new("windows"))
    } else {
        Err(MigError::from_code(MigErrorCode::ErrInvOSType, "invalid OS, not windows",None))
    }
}

pub fn process() -> Result<(),MigError> {
    Err(MigError::from_code(MigErrorCode::ErrNotImpl, &format!("process is not yet implemented for {}",OS),None))    
}