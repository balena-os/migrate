// extern crate wmi;

use log::{error, warn, info, trace};
use wmi::{COMLibrary, WMIConnection};
use std::collections::HashMap;
pub use wmi::Variant;

use failure::{Fail,ResultExt};
use crate::mig_error::{MigError,MigErrorKind,MigErrCtx};


const MODULE: &str = "mswin::wmi_utils";

pub const WMIQ_OS: &str = "SELECT Caption,Version,OSArchitecture, BootDevice, TotalVisibleMemorySize,FreePhysicalMemory FROM Win32_OperatingSystem";
pub const WMIQ_CSProd: &str = "SELECT * FROM Win32_ComputerSystemProduct";
pub const WMIQ_BootConfig: &str = "SELECT * FROM Win32_SystemBootConfiguration";
// pub const WMIQ_Disk: &str = "SELECT * FROM Win32_DiskDrive"; 
pub const WMIQ_Disk: &str = "SELECT Caption,Partitions,Status,DeviceID,Size,BytesPerSector,MediaType,InterfaceType FROM Win32_DiskDrive";
// pub const WMIQ_Partition: &str = "SELECT * FROM Win32_DiskPartition";
pub const WMIQ_Partition: &str = "SELECT Caption,Bootable,Size,NumberOfBlocks,Type,BootPartition,DiskIndex,Index FROM Win32_DiskPartition";
pub struct WmiUtils {
    wmi_con: WMIConnection,
}

impl WmiUtils {
    pub fn new() -> Result<WmiUtils,MigError> {
        trace!("{}::new: entered", MODULE);
        let com_con = COMLibrary::new().context(MigErrCtx::from(MigErrorKind::WmiInit))?;
        Ok(Self {
            wmi_con: WMIConnection::new(com_con.into()).context(MigErrCtx::from(MigErrorKind::WmiInit))?,
        })
    }
    
    pub fn wmi_query(&self,query: &str) -> Result<Vec<HashMap<String, Variant>>, MigError> {    
        trace!("{}::wmi_query: entered with '{}'", MODULE, query);
        Ok(self.wmi_con.raw_query(query).context(MigErrCtx::from(MigErrorKind::WmiQueryFailed))?)
    }       
}
