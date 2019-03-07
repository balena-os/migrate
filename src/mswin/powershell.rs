const MODULE: &str = "win_test::mswin::powershell";

const POWERSHELL: &str = "powershell.exe";

pub const POWERSHELL_GET_CMDLET_PARAMS: [&'static str; 3] =
    ["Get-Command", "-CommandType", "Cmdlet"];
pub const POWERSHELL_SYSINFO_PARAMS: [&'static str; 3] = ["Systeminfo", "/FO", "CSV"];
pub const POWERSHELL_VERSION_PARAMS: [&'static str; 1] = ["$PSVersionTable.PSVersion"];

use crate::common::mig_error::{MigError, MigErrorCode};
use crate::common::SysInfo;

use csv;
use lazy_static::lazy_static;
use log::{error, trace, warn};
use regex::Regex;
use std::collections::HashSet;
use std::io::ErrorKind;
use std::io::Read;
use std::process::{Command, Stdio};

struct PWRes {
    stdout: String,
    stderr: String,
}

pub struct PSInfo {
    ps_ver: Option<(u32, u32)>,
    ps_cmdlets: HashSet<String>,
    si_os_name: String,
    si_os_release: String,
    si_mem_tot: usize,
    si_mem_avail: usize,
    si_boot_dev: String,
}

impl PSInfo {
    pub fn try_init() -> Result<PSInfo, MigError> {
        let mut ps_info = PSInfo {
            ps_ver: None,
            ps_cmdlets: HashSet::new(),
            si_os_name: String::new(),
            si_os_release: String::new(),
            si_mem_tot: 0,
            si_mem_avail: 0,
            si_boot_dev: String::new(),
        };

        match ps_info.get_cmdlets() {
            Ok(_v) => (),
            Err(why) => return Err(why),
        };

        match ps_info.get_ps_ver() {
            Ok(_v) => (),
            Err(why) => return Err(why),
        };

        match ps_info.init_sys_info() {
            Ok(_v) => (),
            Err(why) => return Err(why),
        };

        Ok(ps_info)
    }

    fn has_command(ps_info: &mut PSInfo, cmd: &str) -> bool {
        false
    }

    pub fn get_ps_ver(&mut self) -> Result<(u32, u32), MigError> {
        trace!("{}::get_ps_ver(): called", MODULE);

        match self.ps_ver {
            Some(v) => return Ok(v),
            None => (),
        }

        trace!("{}::get_ps_ver(): calling powershell", MODULE);
        let output = call_to_string(&POWERSHELL_VERSION_PARAMS, true)?;

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
                self.ps_ver = Some((1, 0));
                return Ok(self.ps_ver.unwrap())
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

        self.ps_ver = Some((major, minor));
        Ok(self.ps_ver.unwrap())
    }

    fn get_cmdlets(&mut self) -> Result<usize, MigError> {
        trace!("{}::get_cmdlets(): called", MODULE);
        let output = call_to_string(&POWERSHELL_GET_CMDLET_PARAMS, true)?;

        lazy_static! {
            static ref RE: Regex = Regex::new(r"^-+$").unwrap();
        }

        let mut lines = output.stdout.lines().enumerate();

        let mut name_idx: Option<usize> = None;
        let mut cmds: usize = 0;

        for header in match lines.next() {
            Some(s) => s.1.split_whitespace().enumerate(),
            None => {
                return Err(MigError::from_code(
                    MigErrorCode::ErrInvParam,
                    &format!(
                        "{}::get_cmdlets: 0 output lines received from: powershell Get-Commands",
                        MODULE
                    ),
                    None,
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
            None => return Err(MigError::from_code(MigErrorCode::ErrInvParam, &format!("{}::get_cmdlets: name header not found in output from: powershell Get-Commands",MODULE), None)),
        };

        // potentitally skip line with ----
        match lines.next() {
            Some(s) => {
                let words: Vec<&str> = s.1.split_whitespace().collect();
                match words.get(name_idx) {
                        Some(v) => {
                            if !RE.is_match(v) {
                                if self.ps_cmdlets.insert(String::from(*v)) {
                                    cmds += 1;
                                    trace!("{}::get_cmdlets(): added cmdlet '{}'", MODULE, *v);
                                } else {
                                    warn!("{}::get_cmdlets(): duplicate cmdlet '{}'", MODULE, *v);
                                }
                            }
                        },
                        None => return Err(MigError::from_code(MigErrorCode::ErrInvParam, &format!("{}::get_cmdlets: name value not found in output from: powershell Get-Commands",MODULE), None)),
                    }
            }
            None => return Ok(0),
        }

        for line in lines {
            let words: Vec<&str> = line.1.split_whitespace().collect();
            match words.get(name_idx) {
                Some(v) => 
                    if self.ps_cmdlets.insert(String::from(*v)) {
                        trace!("{}::get_cmdlets(): added cmdlet '{}'", MODULE, *v);
                        cmds += 1;
                    } else {
                        warn!("{}::get_cmdlets(): duplicate cmdlet '{}'", MODULE, *v);
                    },                    
                None => return Err(MigError::from_code(MigErrorCode::ErrInvParam, &format!("{}::get_cmdlets: name value not found in output from: powershell Get-Commands",MODULE), None)),
            };
        }

        Ok(cmds)
    }

    fn init_sys_info(&mut self) -> Result<(), MigError> {
        trace!("{}::init_sys_info(): called", MODULE);

        let output = call_to_string(&POWERSHELL_SYSINFO_PARAMS, true)?;
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

impl SysInfo for PSInfo {
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

fn call_to_string(args: &[&str], trim_stdout: bool) -> Result<PWRes, MigError> {
    // TODO: add option - trim -

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
                    &format!(
                        "{}::call_to_string: failed to execute: powershell Systeminfo /FO CSV",
                        MODULE
                    ),
                    Some(Box::new(why)),
                ));
            }
            _ => {
                return Err(MigError::from_code(
                    MigErrorCode::ErrExecProcess,
                    &format!(
                        "{}::call_to_string: failed to execute: powershell Systeminfo /FO CSV",
                        MODULE
                    ),
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
        stdout: match trim_stdout {
            true => String::from(stdout_str.trim()),
            false => stdout_str,
        },
        stderr: stderr_str,
    })
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
        "gB" => mem * 1024 * 1024 * 1024,
        "MB" => mem * 1024 * 1024,
        "KB" => mem * 1024,
        _ => mem,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_powershell() {
        let s_info = PSInfo::try_init().unwrap();
        assert!(!s_info.get_os_name().is_empty());
        assert!(!s_info.get_os_release().is_empty());
        assert!(!s_info.get_mem_avail() > 0);
        assert!(!s_info.get_mem_tot() > 0);
    }
}
