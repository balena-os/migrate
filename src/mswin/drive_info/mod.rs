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

#[derive(Debug)]
pub enum StorageDevice {
    PhysicalDrive(Rc<PhysicalDriveInfo>),
    HarddiskVolume(Rc<RefCell<HarddiskVolumeInfo>>),
    HarddiskPartition(Rc<RefCell<HarddiskPartitionInfo>>),
    Volume(Rc<RefCell<VolumeInfo>>),
    DriveLetter(Rc<RefCell<DriveLetterInfo>>),
}

pub trait DeviceProps {
    fn get_device_name<'a>(&'a self) -> &'a str;
    fn get_device<'a>(&'a self) -> &'a str;
    fn is_same_device<T: DeviceProps>(&self, other: &T) -> bool {
        self.get_device() == other.get_device()
    }
}

pub fn enumerate_drives(migrator: &mut MSWMigrator) -> Result<HashMap<String, StorageDevice>, MigError> {
    let mut dev_map: HashMap<String, StorageDevice> = HashMap::new();
    let mut hdp_list: Vec<<HarddiskPartitionInfo> = Vec::new();
    let mut hdv_map: HashMap<String,HarddiskVolumeInfo> = HashMap::new();
    let mut dl_map: HashMap<String,DriveLetterInfo> = HashMap::new();
    let mut vol_map: HashMap<String,VolumeInfo> = HashMap::new();    
    let mut pd_map: HashMap<String,Rc<PhysicalDriveInfo>> = HashMap::new();    

    for device in query_dos_device(None)? {
        loop {
            if let Some(hdp) = HarddiskPartitionInfo::try_from_device(&device, migrator)? {
                hdp_list.push(hdp);
                break;
            }

            if let Some(hdv) = HarddiskVolumeInfo::try_from_device(&device)? {
                hdv_list.insert(String::from(hdv.get_device()),hdv);
                break;
            }

            if let Some(dl) = DriveLetterInfo::try_from_device(&device)? {
                dl_list.push(dl);
                break;
            }

            if let Some(vol) = VolumeInfo::try_from_device(&device)? {
                vol_list.push(vol);
                break;
            }

            if let Some(dl) = PhysicalDriveInfo::try_from_device(&device, migrator)? {
                pd_list.push(Rc::new(dl));
                break;
            }

            break;
        }
    }

    loop {
        match hdp_list.pop() {
            Some(hdp) => {
                let mut hdpart = hdp.as_ref().borrow_mut();
                info!("{}::enumerate_drives: looking at: {:?}", MODULE, hdpart);
                let findstr = format!("PhysicalDrive{}", hdpart.get_hd_index());
                if let Some(pd) = dev_map.get(&findstr) {
                    if let StorageDevice::PhysicalDrive(pd) = pd {
                        hdpart.set_phys_disk(pd);
                    } else {
                        panic!(
                            "{}::enumerate_drives: invalid type (not PhysicalDrive) {} in dev_map",
                            MODULE, &findstr
                        );
                    }
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::NotFound,
                        &format!(
                            "{}::enumerate_drives: could not find {} in dev_map",
                            MODULE, &findstr
                        ),
                    ));
                }

                for (idx,hdv) in &hdv_list.enumerate() {
                    if hdpart.is_same_device(&*hdv.as_ref().borrow()) {
                        info!(
                            "{}::enumerate_drives: partition {} found matching hdv {:?}",
                            MODULE,
                            &hdpart.get_device_name(),
                            hdv
                        );
                        hdpart.set_hd_vol(&hdv);
                        hdv.as_ref().borrow_mut().set_hd_part(&hdp);
                        break;
                    }
                }

                if let None = hdpart.get_phys_disk() {
                    warn!(
                        "{}::enumerate_drives: unmatched partition physical disk {:?}",
                        MODULE, hdpart
                    );
                }

                if let None = hdpart.get_hd_vol() {
                    warn!(
                        "{}::enumerate_drives: unmatched partition harddisk volume {:?}",
                        MODULE, hdpart
                    );
                }

                dev_map
                    .entry(String::from(hdpart.get_device_name()))
                    .or_insert(StorageDevice::HarddiskPartition(hdp.clone()));
            }
            None => {
                break;
            }
        }
    }

    loop {
        match vol_list.pop() {
            Some(vol) => {
                let mut volume = vol.as_ref().borrow_mut();
                for hdv in &hdv_list {
                    if volume.is_same_device(&*hdv.as_ref().borrow()) {
                        info!(
                            "{}::enumerate_drives: volume {} found matching hdv {:?}",
                            MODULE,
                            &volume.get_device_name(),
                            hdv
                        );
                        // TODO: modify hd_vol here
                        volume.set_hd_vol(hdv);
                        break;
                    }
                }

                if let None = volume.get_hd_vol() {
                    warn!(
                        "{}::enumerate_drives: unmatched volume {:?}",
                        MODULE, volume
                    );
                }

                dev_map
                    .entry(String::from(volume.get_device_name()))
                    .or_insert(StorageDevice::Volume(vol.clone()));
            }
            None => {
                break;
            }
        }
    }

    loop {
        match dl_list.pop() {
            Some(dl) => {
                let mut driveletter = dl.as_ref().borrow_mut();
                for hdv in &hdv_list {
                    if driveletter.is_same_device(&*hdv.as_ref().borrow()) {
                        info!(
                            "{}::enumerate_drives: driveletter {} found matching hdv {:?}",
                            MODULE,
                            &driveletter.get_device_name(),
                            hdv
                        );
                        // TODO: modify hd_vol here
                        driveletter.set_hd_vol(hdv);
                        break;
                    }
                }
                /*
                if let None = driveletter.hd_vol {
                    warn!("{}::enumerate_drives: unmatched drive letter {:?}", MODULE, driveletter);
                }
                */

                dev_map
                    .entry(String::from(driveletter.get_device_name()))
                    .or_insert(StorageDevice::DriveLetter(dl.clone()));
            }
            None => {
                break;
            }
        }
    }

    loop {
        match hdv_list.pop() {
            Some(hdv) => {
                let hdvol = hdv.as_ref().borrow();
                if let None = hdvol.get_hd_part() {
                    warn!(
                        "{}::enumerate_drives: unmatched harddisk volume {:?}",
                        MODULE, hdvol
                    );
                }
                dev_map
                    .entry(String::from(hdvol.get_device_name()))
                    .or_insert(StorageDevice::HarddiskVolume(hdv.clone()));
            }
            None => {
                break;
            }
        }
    }

    Ok(dev_map)
}
