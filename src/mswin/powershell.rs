const MODULE: &str = "win_test::mswin::powershell";

const POWERSHELL: &str = "powershell.exe";
pub const POWERSHELL_SYSINFO_PARAMS: [&'static str; 3] = ["Systeminfo", "/FO", "CSV"];
pub const POWERSHELL_VERSION_PARAMS: [&'static str; 1] = ["$PSVersionTable.PSVersion"];

use crate::common::mig_error::{MigError, MigErrorCode};

use log::{ warn, trace};
use std::io::ErrorKind;
use std::io::Read;
use std::process::{Command, Stdio};

static mut POWERSHELL_VERSION: Option<(u32,u32)> = None;

pub fn available() -> bool {
    trace!("{}::available(): called available()", MODULE);

    match get_ps_ver() {
        Ok(_v) => true,
        Err(_why) => false
    }
}

pub fn get_ps_ver() -> Result<(u32,u32),MigError>  {
    trace!("{}::get_ps_ver(): called", MODULE);

    // TODO: add mutex

    unsafe {
        match &POWERSHELL_VERSION {
            Some(s) => return Ok(s.clone()),
            None => () 
        }
    }

    let output = call_to_string(&POWERSHELL_VERSION_PARAMS)?;
     
    let lines : Vec<&str> = output.stdout.lines().collect();
    match lines.len() {
        3 => (),
        0 => {
            warn!("{}::get_ps_ver(): no output from command, assuming version 1.0", MODULE);
            unsafe {                
                POWERSHELL_VERSION = Some((1, 0));
            }
            return Ok((1,0))
        }        
        _ => return Err(MigError::from_code(MigErrorCode::ErrInvParam, &format!("{}::available(): unexpected number of ouput lines in powershell version output: {}", MODULE, output.stdout),None))            
    }

    let headers : Vec<&str> = lines[0].split_whitespace().collect();
    let values : Vec<&str> = lines[2].split_whitespace().collect();

    let mut major : u32 = 1;
    let mut minor : u32 = 0;

    for idx in 0..headers.len() {        
        let hdr: &str = &headers[idx];
        match hdr {
            "Major" => { major = values.get(idx).unwrap().parse().unwrap(); }
            "Minor" => { minor = values.get(idx).unwrap().parse().unwrap(); }
            _ => { break; }
            }
        }

    unsafe {
        POWERSHELL_VERSION = Some((major, minor));
    }

    Ok((major,minor))    
}


pub struct PWRes {
    stdout: String,
    stderr: String,
}

fn call_to_string(args: &[&str]) -> Result<PWRes, MigError> {
    let process = match Command::new(POWERSHELL)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(process) => process,
        Err(why) => match why.kind() {
            ErrorKind::NotFound => {
                return Err(MigError::from_code(
                    MigErrorCode::ErrPgmNotFound,
                    "failed to execute: powershell Systeminfo /FO CSV",
                    Some(Box::new(why)),
                ));
            }
            _ => {
                return Err(MigError::from_code(
                    MigErrorCode::ErrExecProcess,
                    "failed to execute: powershell Systeminfo /FO CSV",
                    Some(Box::new(why)),
                ));
            }
        },
    };

    let mut stdout_str: String = String::from("");
    match process.stdout {
        Some(mut sout) => match sout.read_to_string(&mut stdout_str) {
            Ok(_bytes) => (),
            Err(why) => {
                return Err(MigError::from_code(
                    MigErrorCode::ErrCmdIO,
                    "failed to read command output from: powershell Systeminfo /FO CSV",
                    Some(Box::new(why)),
                ));
            }
        },
        None => {
            return Err(MigError::from_code(
                MigErrorCode::ErrCmdIO,
                "failed to read command output from: powershell Systeminfo /FO CSV",
                None,
            ));
        }
    };

    let mut stderr_str: String = String::from("");
    match process.stderr {
        Some(mut serr) => match serr.read_to_string(&mut stderr_str) {
            Ok(_bytes) => (),
            Err(why) => {
                return Err(MigError::from_code(
                    MigErrorCode::ErrCmdIO,
                    "failed to read command error output from: powershell Systeminfo /FO CSV",
                    Some(Box::new(why)),
                ));
            }
        },
        None => {
            return Err(MigError::from_code(
                MigErrorCode::ErrCmdIO,
                "failed to read command error output from: powershell Systeminfo /FO CSV",
                None,
            ));
        }
    };

    Ok(PWRes {
        stdout: stdout_str,
        stderr: stderr_str,
    })
}
