use crate::common::mig_error::{MigError, MigErrorCode};
use crate::common::SysInfo;

use log::{error, trace, info};
// use std::error::Error;
// use std::io::prelude::*;

use csv;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::process::{Command, Stdio};
use std::vec::Vec;

const POWERSHELL : &str  = "powershell.exe";
const POWERSHELL_PARAMS : [&'static str;3] = ["Systeminfo", "/FO", "CSV"];


// const OS: &str = "windows";

pub fn available() -> bool {
    trace!("called available()");
    return cfg!(windows);
}

pub fn sys_info() -> Result<SysInfo, MigError> {
    trace!("called sys_info()");
    if cfg!(windows) {
        // Spawn the command `powershell Systeminfo /FO CSV`
        let process = match Command::new(POWERSHELL)
            .args(&POWERSHELL_PARAMS)
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

        let mut reader = match process.stdout {
            Some(std_out) => csv::Reader::from_reader(std_out),
            None => {
                return Err(MigError::from_code(
                    MigErrorCode::ErrCmdIO,
                    "failed to read command output from: powershell Systeminfo /FO CSV",
                    None,
                ));
            }
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

        let headers =
            match reader.headers() {
                Ok(sr) => {
                    let hdrs: Vec<&str> = sr.iter().collect();
                    hdrs
                }
                Err(_why) => return Err(MigError::from_code(
                    MigErrorCode::ErrInvParam,
                    "no headers found in output lines received from: powershell Systeminfo /FO CSV",
                    None,
                )), // Some(Box::new(why))))
            };

        let data =
            match &records[0] {
                Ok(sr) => {
                    let dt: Vec<&str> = sr.iter().collect();
                    dt
                }
                Err(_why) => return Err(MigError::from_code(
                    MigErrorCode::ErrInvParam,
                    "no data found in output lines received from: powershell Systeminfo /FO CSV",
                    None,
                )), // Some(Box::new(why))))
            };

        trace!("sys_info: headers: {:?}", headers);
        trace!("sys_info: data:    {:?}", data);

        let mut sys_info_map: HashMap<String, String> = HashMap::new();
        let columns = headers.len();

        for idx in 0..columns {
            let hdr: &str = &headers[idx];
            let data_str = match data.get(idx) {
                Some(s) => s,
                None => "",
            };
            trace!("sys_info: adding {} ->  {}", hdr, data_str);
            sys_info_map.insert(String::from(hdr), String::from(data_str));
        }

        let mut s_info = SysInfo::new(sys_info_map);
        s_info.set_os_name("OS Name")?;

        Ok(s_info)
    } else {
        Err(MigError::from_code(
            MigErrorCode::ErrInvOSType,
            "invalid OS, not windows",
            None,
        ))
    }
}

pub fn process() -> Result<(), MigError> {
    let s_info = sys_info()?;    
    info!("process: os_type = {}", s_info.get_os_name());
    Ok(())
}

#[cfg(test)]
#[test]
fn get_sys_info() {
    let s_info = sys_info().unwrap();
    let os_name = s_info.get_os_name();
    assert!(os_name.len() > 0);
}

