mod powershell;

use crate::common::mig_error;
use crate::common::SysInfo;


use std::process::{Command};
use log::{warn, trace, error};
use csv;
use lazy_static::lazy_static;
use regex::Regex;

use mig_error::{MigError, MigErrorCode};
use powershell::PSInfo;

// use std::error::Error;
// use std::io::prelude::*;




const MODULE: &str = "mswin";
const SYSINFO_CMD: &str = "systeminfo";
const SYSINFO_ARGS: [&str;2] = ["/FO","CSV"];
// const OS: &str = "windows";

pub struct MSWInfo {
    ps_info: Option<PSInfo>,
    si_os_name: String,
    si_os_release: String,
    si_mem_tot: usize,
    si_mem_avail: usize,
    si_boot_dev: String,
}

impl MSWInfo {
    pub fn try_init() -> Result<MSWInfo, MigError> {
        if !cfg!(windows) {
            return Err(MigError::from_code(MigErrorCode::ErrInvOSType, &format!("{}: this module only works on Windows OS ", MODULE), None)); 
        }

        let mut msw_info = MSWInfo {
            ps_info: None,
            si_os_name: String::new(),
            si_os_release: String::new(),
            si_mem_tot: 0,
            si_mem_avail: 0,
            si_boot_dev: String::new(),
        };

        match msw_info.init_sys_info() {
            Ok(_v) => (),
            Err(why) => return Err(why),
        };

        // TODO: 
        msw_info.ps_info = match PSInfo::try_init() {
            Ok(pi) => Some(pi), 
            Err(why) => {
                warn!("{}::try_init: failed to initialize powershell: {:?}", MODULE, why);
                None    
            },
        };

        Ok(msw_info)
    }

    fn init_sys_info(&mut self) -> Result<(), MigError> {
        trace!("{}::init_sys_info(): called", MODULE);

        let output = match Command::new(SYSINFO_CMD)
            .args(&SYSINFO_ARGS)
            .output() {            
            Ok(o) => o,
            Err(why) => return Err( MigError::from_code(
                                    MigErrorCode::ErrExecProcess,
                                    &format!(
                                        "{}::call_to_string: failed to execute: powershell Systeminfo /FO CSV",
                                        MODULE),
                                    Some(Box::new(why)),))
            };

            if !output.status.success() {                
                return Err(MigError::from_code(MigErrorCode::ErrExecProcess, &format!("{}::init_sys_info: command failed with exit code {}", MODULE, output.status.code().unwrap_or(0)), None));
            }

            let mut reader = csv::Reader::from_reader(output.stdout.as_slice());
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
                    )), // TODO: Some(Box::new(why))))
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
                        None
                    )),
                };

            trace!("sys_info: headers: {:?}", headers);
            trace!("sys_info: data:    {:?}", data);

            let columns = headers.len();

            for idx in 0..columns {
                let hdr = &headers[idx];
                /*
                Host Name ->  WINDELN
                OS Name ->  Microsoft Windows 7 Ultimate N
                OS Version ->  6.1.7600 N/A Build 7600
                OS Manufacturer ->  Microsoft Corporation
                OS Configuration ->  Standalone Workstation
                OS Build Type ->  Multiprocessor Free
                Registered Owner ->  thomas
                Registered Organization ->
                Product ID ->  00432-076-1125903-86398
                Original Install Date ->  1/12/2018, 9:19:19 p.m.
                System Boot Time ->  8/03/2019, 8:19:16 a.m.
                System Manufacturer ->  innotek GmbH
                System Model ->  VirtualBox
                System Type ->  x64-based PC
                Processor(s) ->  1 Processor(s) Installed.,[01]: Intel64 Family 6 Model 78 Stepping 3 GenuineIntel ~2593 Mhz
                BIOS Version ->  innotek GmbH VirtualBox, 1/12/2006
                Windows Directory ->  C:\Windows
                System Directory ->  C:\Windows\system32
                Boot Device ->  \Device\HarddiskVolume1
                System Locale ->  en-nz;English (New Zealand)
                Input Locale ->  de;German (Germany)
                Time Zone ->  (UTC+12:00) Auckland, Wellington
                Total Physical Memory ->  4,096 MB
                Available Physical Memory ->  3,309 MB
                Virtual Memory: Max Size ->  8,189 MB
                Virtual Memory: Available ->  7,360 MB
                Virtual Memory: In Use ->  829 MB
                Page File Location(s) ->  C:\pagefile.sys
                Domain ->  WORKGROUP
                Logon Server ->  N/A
                Hotfix(s) ->  4 Hotfix(s) Installed.,[01]: KB968771,[02]: KB958488,[03]: KB974039,[04]: KB974940
                Network Card(s) ->  1 NIC(s) Installed.,[01]: Intel(R) PRO/1000 MT Desktop Adapter,      Connection Name: Local Area Connection,      DHCP Enabled:    Yes,      DHCP Ser
                ver:     192.168.1.2,      IP address(es),      [01]: 192.168.1.42,      [02]: fe80::1527:8144:4e3d:816b
                */

                match *hdr {
                    "OS Name" => {
                        self.si_os_name = get_str_value(data.get(idx))?;
                    }
                    "OS Version" => {
                        self.si_os_release = get_str_value(data.get(idx))?;
                    }
                    "Total Physical Memory" => {
                        self.si_mem_tot = parse_mem_value(data.get(idx))?;
                    }
                    "Available Physical Memory" => {
                        self.si_mem_avail = parse_mem_value(data.get(idx))?;
                    }
                    "Boot Device" => {
                        self.si_boot_dev = get_str_value(data.get(idx))?;
                    }

                    _ => (),
                };
            }

        Ok(())
    }

}  

impl SysInfo for MSWInfo {
    fn get_os_name(&self) -> String {
        self.si_os_name.clone()
    }

    fn get_os_release(&self) -> String {
        self.si_os_release.clone()
    }

    fn get_mem_tot(&self) -> usize {
        self.si_mem_tot
    }

    fn get_mem_avail(&self) -> usize {
        self.si_mem_avail
    }

    fn get_boot_dev(&self) -> String {
        self.si_boot_dev.clone()
    }
}


pub fn available() -> bool {
    trace!("called available()");
    return cfg!(windows);
}

pub fn process() -> Result<(), MigError> {
    let mut ps_info = powershell::PSInfo::try_init()?;
    // info!("process: os_type = {}", ps_info.get_os_name());
    Ok(())
}

fn get_str_value(val: Option<&&str>) -> Result<String, MigError> {
    match val {
        Some(s) => Ok(String::from(*s)),
        None => {
            return Err(MigError::from_code(
                MigErrorCode::ErrNotFound,
                &format!("{}::get_str_value: empty value", MODULE),
                None,
            ));
        }
    }
}

fn parse_mem_value(val: Option<&&str>) -> Result<usize, MigError> {
    let val = match val {
        Some(s) => *s,
        None => {
            return Err(MigError::from_code(
                MigErrorCode::ErrNotFound,
                &format!("{}::parse_mem_value: empty value", MODULE),
                None,
            ));
        }
    };

    trace!("{}::parse_mem_value: entered with {}", MODULE, val);

    lazy_static! {
        static ref RE: Regex = Regex::new(r"^((\d{1,3}(,\d{3})*)|(\d+))\s*(MB|KB|GB)?$").unwrap();
        //static ref RE: Regex = Regex::new(r"^(\d+)\s*(MB|KB|GB)?$").unwrap();
    }

    let captures = match RE.captures(val) {
        Some(c) => c,
        None => {
            return Err(MigError::from_code(
                MigErrorCode::ErrInvParam,
                &format!(
                "{}::parse_mem_value: failed to parse memory field from '{}' - no match on regex",
                MODULE, val
            ),
                None,
            ));
        }
    };

    let digits = match captures.get(2) {
        Some(m) => m.as_str().replace(",", ""),
        None => match captures.get(4) {
            Some(m) => String::from(m.as_str()),
            None => {
                return Err(MigError::from_code(
                    MigErrorCode::ErrInvParam,
                    &format!(
                    "{}::parse_mem_value: failed to parse memory from '{}' - no digits captured ",
                    MODULE, val
                ),
                    None,
                ));
            }
        },
    };

    trace!("get_mem_value: got digits {}", digits);

    let mem: usize  = match digits.parse() {
        Ok(num) => num,
        Err(why) => return Err(MigError::from_code(MigErrorCode::ErrInvParam, &format!("{}::parse_mem_value: failed to parse memory from '{}' - failed to parse digits '{}'", MODULE, val, digits) , Some(Box::new(why)))),
    };

    Ok(match captures.get(2).map_or("", |m| m.as_str()) {
        "GB" => mem * 1024 * 1024 * 1024,
        "MB" => mem * 1024 * 1024,
        "KB" => mem * 1024,
        _ => mem,
    })
}
