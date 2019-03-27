use log::{info, warn};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::mswin::win_api::query_dos_device;
use crate::mswin::MSWMigrator;
use crate::{MigError, MigErrorKind};

pub mod driveletter;
pub mod hd_partition;
pub mod hd_volume;
pub mod phys_drive;
pub mod volume;

pub use driveletter::DriveLetterInfo;
pub use hd_partition::HarddiskPartitionInfo;
pub use hd_volume::HarddiskVolumeInfo;
pub use phys_drive::PhysicalDriveInfo;
pub use volume::VolumeInfo;

const MODULE: &str = "mswin::drive_info";

pub trait DeviceProps {
    fn get_device_name<'a>(&'a self) -> &'a str;
    fn get_device<'a>(&'a self) -> &'a str;
    fn is_same_device<T: DeviceProps>(&self, other: &T) -> bool {
        self.get_device() == other.get_device()
    }
}

pub fn enumerate_drives(migrator: &mut MSWMigrator) -> Result<HashMap<u64,PhysicalDriveInfo>, MigError> {    
    let mut hdp_list: Vec<HarddiskPartitionInfo> = Vec::new();
    let mut hdv_map: HashMap<String,HarddiskVolumeInfo> = HashMap::new();
    let mut dl_map: HashMap<String,DriveLetterInfo> = HashMap::new();
    let mut vol_map: HashMap<String,VolumeInfo> = HashMap::new();    
    let mut pd_map: HashMap<u64,PhysicalDriveInfo> = HashMap::new();    

    for device in query_dos_device(None)? {
        loop {
            if let Some(hdp) = HarddiskPartitionInfo::try_from_device(&device, migrator)? {
                hdp_list.push(hdp);
                break;
            }

            if let Some(hdv) = HarddiskVolumeInfo::try_from_device(&device)? {
                hdv_map.insert(String::from(hdv.get_device()),hdv);
                break;
            }

            if let Some(dl) = DriveLetterInfo::try_from_device(&device)? {
                dl_map.insert(String::from(dl.get_device()),dl);
                break;
            }

            if let Some(vol) = VolumeInfo::try_from_device(&device)? {
                vol_map.insert(String::from(vol.get_device()),vol);
                break;
            }

            if let Some(pd) = PhysicalDriveInfo::try_from_device(&device, migrator)? {
                pd_map.insert(pd.get_index(),pd);
                break;
            }

            break;
        }
    }

    for mut hdpart in hdp_list {            
        info!("{}::enumerate_drives: looking at: {:?}", MODULE, hdpart);        
        if let Some(hdv) = hdv_map.remove(hdpart.get_device()) {
            hdpart.set_hd_vol(hdv);
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "{}::enumerate_drives: could not find {} in harddisk volume map",
                    MODULE, hdpart.get_device()
                ),
            ));
        } 

        if let Some(dl) = dl_map.remove(hdpart.get_device()) {
            hdpart.set_driveletter(dl);
        } 

        if let Some(vol) = vol_map.remove(hdpart.get_device()) {
            hdpart.set_volume(vol);
        } 
        
        //pd_map.entry(hdpart.get_hd_index()).and_modify(|pd| { *pd.add_partition(hdpart)?; });
        
        if let Some(pd) = pd_map.get_mut(&hdpart.get_hd_index()) {
            if (pd.get_partitions() as u64)  >= hdpart.get_part_index()  {
                pd.add_partition(hdpart)?;
            } else {
                warn!("{}::enumerate_drives: ignoring invalid partition {}", MODULE, hdpart.get_device_name())
            }            
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "{}::enumerate_drives: could not find {} in physical drive map",
                    MODULE, hdpart.get_hd_index()
                ),
            ));
        }
    }

    Ok(pd_map)
}

