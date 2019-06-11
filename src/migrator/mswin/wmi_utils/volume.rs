use crate::{
    defs::{FileSystem},
    common::{MigError, MigErrorKind,},
    mswin::win_api::{query_dos_device, wmi_api::WmiAPI},
};
use log::debug;

use super::{Partition, QueryRes, NS_CVIM2};

const QUERY_ALL: &str =
    "SELECT Name, DeviceID, BlockSize, BootVolume, Capacity, FileSystem, FreeSpace, \
     SystemVolume, MaximumFileNameLength, PageFilePresent, Label, DriveType, DriveLetter \
     FROM Win32_Volume";

#[derive(Debug, Clone)]
pub(crate) enum DriveType {
    Unknown,
    NoRootDir,
    RemovableDisk,
    LocalDisk,
    NetworkDrive,
    CompactDisk,
    RamDisk,
}

impl DriveType {
    pub fn from_u32(val: u32) -> DriveType {
        match val {
            0 => DriveType::Unknown,
            1 => DriveType::NoRootDir,
            2 => DriveType::RemovableDisk,
            3 => DriveType::LocalDisk,
            4 => DriveType::NetworkDrive,
            5 => DriveType::CompactDisk,
            6 => DriveType::RamDisk,
            _ => DriveType::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Volume {
    name: String,
    device_id: String,
    label: String,
    drive_letter: String,
    file_system: FileSystem,
    // device: String,
    boot_volume: bool,
    system_volume: bool,
    page_file_present: bool,
    block_size: Option<u64>,
    capacity: Option<u64>,
    free_space: Option<u64>,
    max_filename_length: Option<u64>,
    drive_type: DriveType,
}

impl<'a> Volume {
    /*    pub fn get_query_all() -> &'static str {
            QUERY_ALL
        }
    */

    pub fn query_all() -> Result<Vec<Volume>, MigError> {
        let query = QUERY_ALL;
        debug!("query_volumes: performing WMI Query: '{}'", query);
        let q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(query)?;
        let mut result: Vec<Volume> = Vec::new();
        for res in q_res {
            let res_map = QueryRes::new(&res);
            result.push(Volume::new(res_map)?);
        }
        Ok(result)
    }

    pub fn query_by_drive_letter(dl: &str) -> Result<Volume, MigError> {
        let query = format!("{} WHERE DriveLetter={}", QUERY_ALL, dl);
        debug!("query_volumes: performing WMI Query: '{}'", query);
        let q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(&query)?;
        if q_res.len() == 1 {
            Ok(Volume::new(QueryRes::new(&q_res[0]))?)
        } else {
            Err(MigError::from_remark(MigErrorKind::InvParam, &format!("Invalid result count: {}", q_res.len())))
        }
    }

    pub fn query_by_device_id(device_id: &str) -> Result<Volume, MigError> {
        let query = format!("{} WHERE DeviceID={}", QUERY_ALL, device_id);
        debug!("query_volumes: performing WMI Query: '{}'", query);
        let q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(&query)?;
        if q_res.len() == 1 {
            Ok(Volume::new(QueryRes::new(&q_res[0]))?)
        } else {
            Err(MigError::from_remark(MigErrorKind::InvParam, &format!("Invalid result count: {}", q_res.len())))
        }
    }


    pub fn query_system_volumes() -> Result<Vec<Volume>, MigError> {
        let query = format!("{} WHERE SystemVolume=True", QUERY_ALL);
        debug!("query_volumes: performing WMI Query: '{}'", query);
        let q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(&query)?;
        let mut result: Vec<Volume> = Vec::new();
        for res in q_res {
            let res_map = QueryRes::new(&res);
            result.push(Volume::new(res_map)?);
        }
        Ok(result)
    }

    pub fn query_boot_volumes() -> Result<Vec<Volume>, MigError> {
        let query = format!("{} WHERE BootVolume=True", QUERY_ALL);
        debug!("query_volumes: performing WMI Query: '{}'", query);
        let q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(&query)?;
        let mut result: Vec<Volume> = Vec::new();
        for res in q_res {
            let res_map = QueryRes::new(&res);
            result.push(Volume::new(res_map)?);
        }
        Ok(result)
    }

    pub(crate) fn new(res_map: QueryRes) -> Result<Volume, MigError> {
        let device_id = String::from(res_map.get_string_property("DeviceID")?);

        /*
        let handle = device_id
            .trim_start_matches(r#"\\?\"#)
            .trim_end_matches(r#"\"#);
        debug!("'{}' -> handle: '{}'", device_id, handle);
        // let device = query_dos_device(Some(handle))?.get(0).unwrap().clone();
        */

        Ok(Volume {
            name: String::from(res_map.get_string_property("Name")?),
            device_id,
            // device,
            label: String::from(res_map.get_string_property("Label")?),
            file_system: FileSystem::from_str(res_map.get_string_property("FileSystem")?),
            drive_letter: String::from(res_map.get_string_property("DriveLetter")?),
            boot_volume: res_map.get_bool_property_with_def("BootVolume", false)?,
            system_volume: res_map.get_bool_property_with_def("SystemVolume", false)?,
            page_file_present: res_map.get_bool_property_with_def("PageFilePresent", false)?,
            block_size: res_map.get_optional_uint_property("BlockSize")?,
            capacity: res_map.get_optional_uint_property("Capacity")?,
            free_space: res_map.get_optional_uint_property("FreeSpace")?,
            max_filename_length: res_map.get_optional_uint_property("MaximumFileNameLength")?,
            drive_type: DriveType::from_u32(res_map.get_uint_property("DriveType")? as u32),
        })
    }

    pub fn is_boot(&self) -> bool {
        self.boot_volume
    }

    pub fn is_system(&self) -> bool {
        self.system_volume
    }

    pub fn get_name(&'a self) -> &'a str {
        &self.name
    }

    pub fn get_file_system(&'a self) -> &'a FileSystem {
        &self.file_system
    }

    pub fn get_device_id(&'a self) -> &'a str {
        &self.device_id
    }

    /* pub fn get_device(&'a self) -> &'a str {
        &self.device
    }*/

    pub fn get_drive_letter(&'a self) -> &'a str {
        &self.drive_letter
    }
}

/*
PSComputerName               : DESKTOP-AJVE610
__GENUS                      : 2
__CLASS                      : Win32_Volume
__SUPERCLASS                 : CIM_StorageVolume
__DYNASTY                    : CIM_ManagedSystemElement
__RELPATH                    : Win32_Volume.DeviceID="\\\\?\\Volume{523d4064-b421-4b2e-ba0e-320263dcbd27}\\"
__PROPERTY_COUNT             : 44
__DERIVATION                 : {CIM_StorageVolume, CIM_StorageExtent, CIM_LogicalDevice, CIM_LogicalElement...}
__SERVER                     : DESKTOP-AJVE610
__NAMESPACE                  : root\cimv2
__PATH                       : \\DESKTOP-AJVE610\root\cimv2:Win32_Volume.DeviceID="\\\\?\\Volume{523d4064-b421-4b2e-ba0e-320263dcbd27}\\"
Access                       :
Automount                    : True
Availability                 :
BlockSize                    : 1024
BootVolume                   : False
Capacity                     : 99614720
Caption                      : \\?\Volume{523d4064-b421-4b2e-ba0e-320263dcbd27}\
Compressed                   :
ConfigManagerErrorCode       :
ConfigManagerUserConfig      :
CreationClassName            :
Description                  :
DeviceID                     : \\?\Volume{523d4064-b421-4b2e-ba0e-320263dcbd27}\
DirtyBitSet                  :
DriveLetter                  :
DriveType                    : 3
ErrorCleared                 :
ErrorDescription             :
ErrorMethodology             :
FileSystem                   : FAT32
FreeSpace                    : 36080640
IndexingEnabled              :
InstallDate                  :
Label                        :
LastErrorCode                :
MaximumFileNameLength        : 255
Name                         : \\?\Volume{523d4064-b421-4b2e-ba0e-320263dcbd27}\
NumberOfBlocks               :
PageFilePresent              : False
PNPDeviceID                  :
PowerManagementCapabilities  :
PowerManagementSupported     :
Purpose                      :
QuotasEnabled                :
QuotasIncomplete             :
QuotasRebuilding             :
SerialNumber                 : 510402211
Status                       :
StatusInfo                   :
SupportsDiskQuotas           : False
SupportsFileBasedCompression : False
SystemCreationClassName      :
SystemName                   :
SystemVolume                 : True
Scope                        : System.Management.ManagementScope
Path                         : \\DESKTOP-AJVE610\root\cimv2:Win32_Volume.DeviceID="\\\\?\\Volume{523d4064-b421-4b2e-ba0e-320263dcbd27}\\"
Options                      : System.Management.ObjectGetOptions
ClassPath                    : \\DESKTOP-AJVE610\root\cimv2:Win32_Volume
Properties                   : {Access, Automount, Availability, BlockSize...}
SystemProperties             : {__GENUS, __CLASS, __SUPERCLASS, __DYNASTY...}
Qualifiers                   : {dynamic, locale, provider}
Site                         :
Container                    :
*/
