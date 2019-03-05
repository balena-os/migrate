use  crate::common::mig_error::{MigError,MigErrorCode};
use  crate::common::SysInfo;

use log::{trace,error};
// use std::error::Error;
// use std::io::prelude::*;
use std::collections::HashMap;
use std::process::{Command, Stdio};
use std::vec::Vec;
use csv;

// const OS: &str = "windows";

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

    let mut reader = match process.stdout {
        // TODO: how to return why as source source
        None => { return Err(MigError::from_code(MigErrorCode::ErrCmdIO, "failed to read command output from: powershell Systeminfo /FO CSV", None )) }
        Some(std_out) => csv::Reader::from_reader(std_out)
        };

    let records: Vec<csv::Result<csv::StringRecord>> = reader.records().collect();
    match records.len() {
        1 => (),            
        _ => { 
            error!("sys_info: invalid number of records () in command output of  powershell Systeminfo /FO CSV");
            for record in records {
                error!("sys_info: {:?}", record);
            }
            return Err(MigError::from_code(MigErrorCode::ErrInvParam, "unexpected number of output lines received from: powershell Systeminfo /FO CSV", None)); 
        }
    }

    let headers = match reader.headers() {
        Ok(sr) => { 
            let hdrs: Vec<&str> = sr.iter().collect();
            hdrs
        },
        Err(_why) => return Err(MigError::from_code(MigErrorCode::ErrInvParam, "no headers found in output lines received from: powershell Systeminfo /FO CSV", None)) // Some(Box::new(why))))
    };

    let data = match &records[0] {
        Ok(sr) => {
            let dt : Vec<&str> = sr.iter().collect();
            dt
        },
        Err(_why) => return Err(MigError::from_code(MigErrorCode::ErrInvParam, "no data found in output lines received from: powershell Systeminfo /FO CSV", None)) // Some(Box::new(why))))
    };

    trace!("sys_info: headers: {:?}",headers);
    trace!("sys_info: data:    {:?}",data);

    let mut sys_info_map: HashMap<String,String> = HashMap::new();  
    let columns = headers.len();

    for idx in 0..columns {
        let data_str = match data.get(idx) {
            Some(s) => s,
            None => ""
        };
        trace!("sys_info: adding {}", headers[idx]);
        trace!("sys_info: data   {}", data_str);
        sys_info_map.insert(String::from(headers[idx]), String::from(data_str));
    }
    
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