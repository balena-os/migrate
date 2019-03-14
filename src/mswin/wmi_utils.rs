// extern crate wmi;

use log::{error, warn, info, trace};
use wmi::{COMLibrary, WMIConnection};
use std::collections::HashMap;
pub use wmi::Variant;

use crate::common::mig_error::{MigError,MigErrorCode};

const MODULE: &str = "mswin::wmi_utils";

const WMI_QUERY_OS: &str = "SELECT * FROM Win32_OperatingSystem";

pub struct WmiUtils {
    wmi_con: WMIConnection,
}

impl WmiUtils {
    pub fn new() -> Result<WmiUtils,MigError> {
        trace!("{}::new: entered", MODULE);
        let com_con = match COMLibrary::new() {
            Ok(com_con) => com_con,
            Err(_why) => return Err(
                MigError::from_code(
                    MigErrorCode::ErrComInit, 
                    &format!("{}::new: failed to initialize COM interface",MODULE),
                    None)), //Some(Box::new(why))),
            };

        Ok(Self {
            wmi_con: match WMIConnection::new(com_con.into()) {
                Ok(c) => c,
                Err(_why) => return Err(
                    MigError::from_code(
                        MigErrorCode::ErrWmiInit, 
                        &format!("{}::new: failed to initialize WMI interface",MODULE),
                        None)), //Some(Box::new(why))),

            },
        })
    }
    
    pub fn wmi_query_system(&self) -> Result<HashMap<String, Variant>, MigError> {    
        trace!("{}::wmi_query_system: entered", MODULE);

        match self.wmi_con.raw_query(WMI_QUERY_OS) {
            Ok(mut res) => Ok(res.remove(0)),
            Err(why) => { 
                error!("{}::wmi_query_system: failed on query {} : {:?}",MODULE, WMI_QUERY_OS,why);
                return Err(                    
                    MigError::from_code(
                        MigErrorCode::ErrWmiQueryFailed, 
                        &format!("{}::wmi_query_system: failed on query {}",MODULE, WMI_QUERY_OS),
                        None)); //Some(Box::new(why))),
                },
        }
    }       
}
