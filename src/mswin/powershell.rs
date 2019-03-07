const MODULE: &str = "win_test::mswin::powershell";

const POWERSHELL: &str = "powershell.exe";
pub const POWERSHELL_SYSINFO_PARAMS: [&'static str; 3] = ["Systeminfo", "/FO", "CSV"];
pub const POWERSHELL_VERSION_PARAMS: [&'static str; 1] = ["$PSVersionTable.PSVersion"];

use crate::common::mig_error::{MigError, MigErrorCode};
use crate::common::SysInfo;

use lazy_static::lazy_static;
use csv;
use log::{info, error, trace, warn};
use std::collections::HashMap;
use std::io::ErrorKind;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::Mutex;

// static mut POWERSHELL_VERSION: Option<(u32, u32)> = None;

// static mut POWERSHELL_VERSION: Mutex<Option<(u32, u32)>> = Mutex::new(None);


pub fn available() -> bool {
    trace!("{}::available: called available()", MODULE);

    match get_ps_ver() {
        Ok(_v) => true,
        Err(why) => { 
            error!("{}::available: get_ps_ver() returned error: {:?}",MODULE, why);
            false 
            },
    }
}

pub fn get_ps_ver() -> Result<(u32, u32), MigError> {
    trace!("{}::get_ps_ver(): called", MODULE);

    lazy_static! {
        static ref POWERSHELL_VERSION: Mutex<Option<(u32,u32)>> = Mutex::new(None);
    }

    
    let mut version = match POWERSHELL_VERSION.lock() {
        Ok(v) => v,
        Err(why) => return Err(MigError::from_code(MigErrorCode::ErrInvParam, &format!("{}::get_ps_ver: module mutex is poisoned", MODULE),Some(Box::new(why))))
    };

    trace!("{}::get_ps_ver(): after mutex lock {:?}", MODULE, version);

    match *version {
            Some(s) => {
                info!("{}::get_ps_ver(): returning version {:?}", MODULE, version);
                return Ok(s.clone())
            },
            None => (),
    }

    trace!("{}::get_ps_ver(): calling powershell", MODULE);
    let output = call_to_string(&POWERSHELL_VERSION_PARAMS)?;

    trace!("{}::get_ps_ver(): powershell stdout: {}", MODULE, output.stdout);
    trace!("{}::get_ps_ver(): powershell stderr {}", MODULE, output.stderr);

    let mut lines: Vec<&str> = Vec::new();

    for line in output.stdout.lines() {
        if ! line.trim().is_empty() {
            lines.push(line);
        }
    }

    trace!("{}::get_ps_ver(): powershell stdout: lines: {}", MODULE, lines.len());
    match lines.len() {
        3 => (),
        0 => {
            warn!("{}::get_ps_ver(): no output from command, assuming version 1.0", MODULE);
            *version = Some((1, 0));
            return Ok((1,0))
        }        
        _ => return Err(MigError::from_code(MigErrorCode::ErrInvParam, &format!("{}::available(): unexpected number of ouput lines in powershell version output: {}", MODULE, output.stdout),None))
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

    *version = Some((major, minor));    
    Ok((major, minor))
    
}

pub fn sys_info() -> Result<SysInfo, MigError> {
    trace!("{}::sys_info(): called", MODULE);
    if available() {
        let output = call_to_string(&POWERSHELL_SYSINFO_PARAMS)?;
        let mut reader = csv::Reader::from_reader(output.stdout.as_bytes());
        let records: Vec<csv::Result<csv::StringRecord>> = reader.records().collect();
        match records.len() {
            1 => (),
            _ => {
                error!("sys_info: invalid number of records () in command output of  powershell Systeminfo /FO CSV");
                for record in records {
                    error!("sys_info: {:?}", record);
                }
                return Err(MigError::from_code(MigErrorCode::ErrInvParam, &format!("{}::sys_info: unexpected number of output lines received from: powershell Systeminfo /FO CSV",MODULE), None));
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
                    &format!("{}::sys_info: no headers found in output lines received from: powershell Systeminfo /FO CSV",MODULE),
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
                    &format!("{}::sys_info: no data found in output lines received from: powershell Systeminfo /FO CSV",MODULE),
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
        s_info.set_os_version("OS Version")?;
        s_info.set_host_name("Host Name")?;
        s_info.set_tot_mem("Total Physical Memory")?;
        s_info.set_avail_mem("Available Physical Memory")?;

        Ok(s_info)
    } else {
        Err(MigError::from_code(
            MigErrorCode::ErrFeatureMissing,
            &format!("{}::sys_info: powershell not is available on windows",MODULE),
            None,
        ))
    }
}

pub struct PWRes {
    stdout: String,
    stderr: String,
}

fn call_to_string(args: &[&str]) -> Result<PWRes, MigError> {
    trace!("{}::call_to_string(): called with {:?}", MODULE, args);
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
                    &format!("{}::call_to_string: failed to execute: powershell Systeminfo /FO CSV",MODULE),
                    Some(Box::new(why)),
                ));
            }
            _ => {
                return Err(MigError::from_code(
                    MigErrorCode::ErrExecProcess,
                    &format!("{}::call_to_string: failed to execute: powershell Systeminfo /FO CSV",MODULE),
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
                    &format!("{}::call_to_string: failed to read command output from: powershell Systeminfo /FO CSV",MODULE),
                    Some(Box::new(why)),
                ));
            }
        },
        None => {
            return Err(MigError::from_code(
                MigErrorCode::ErrCmdIO,
                &format!("{}::call_to_string: failed to read command output from: powershell Systeminfo /FO CSV",MODULE),
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
                    &format!("{}::call_to_string: failed to read command error output from: powershell Systeminfo /FO CSV",MODULE),
                    Some(Box::new(why)),
                ));
            }
        },
        None => {
            return Err(MigError::from_code(
                MigErrorCode::ErrCmdIO,
                &format!("{}::call_to_string: failed to read command error output from: powershell Systeminfo /FO CSV",MODULE),
                None,
            ));
        }
    };

    Ok(PWRes {
        stdout: stdout_str,
        stderr: stderr_str,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] 
    fn is_available() {
        available();
    }

    #[test]
    fn get_sys_info() {
        if available() {
            let s_info = sys_info().unwrap();
            let os_name = s_info.get_os_name();
            assert!(os_name.len() > 0);
        }
    }
}
