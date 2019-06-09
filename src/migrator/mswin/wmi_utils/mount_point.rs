
use log::debug;
use std::path::{PathBuf, Path};
use lazy_static::{lazy_static};


// use log::{debug};
use super::QueryRes;
use crate::{
    defs::{FileSystem},
    common::{MigError, MigErrorKind},
    mswin::{
        win_api::wmi_api::WmiAPI,
        wmi_utils::volume::Volume,
    },
};

use crate::mswin::wmi_utils::NS_CVIM2;


const QUERY_ALL: &str = "SELECT Directory,Volume FROM Win32_MountPoint";

const QUERY_VOL2DIR: &str = r#"SELECT Directory FROM Win32_MountPoint where Volume='Win32_Volume.DeviceID=""'"#;


pub(crate) struct MountPoint {
    directory: PathBuf,
    volume: Volume
}


impl<'a>  MountPoint {
    pub fn query_all() -> Result<Vec<MountPoint>, MigError> {
        Ok(MountPoint::from_query(QUERY_ALL)?)
    }

    pub fn query_directory_by_volume(volume: &Volume) -> Result<Option<PathBuf>, MigError> {
        let vol_id = volume.get_device_id();
        for mount_point in MountPoint::query_all()? {
            if mount_point.volume.get_device_id() == vol_id {
                return Ok(Some(mount_point.directory))
            }
        }
        OK(None)
    }


    fn from_query(query: &str) -> Result<Vec<MountPoint>, MigError> {
        let q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(query)?;
        let mut result: Vec<MountPoint> = Vec::new();
        for res in q_res {
            // expected
            //  Directory        : Win32_Directory.Name="C:\\"
            //  Volume           : Win32_Volume.DeviceID="\\\\?\\Volume{927a901b-d6fe-4133-a909-11b2ec00d54a}\\"

            let res_map = QueryRes::new(&res);

            let res_str = res_map.get_string_property("Directory")?;
            debug!("res_str Directory: '{}'", res_str);
            let parts: Vec<&str> = res_str.split("=").collect();            
            let directory = if parts.len() == 2 {
                parts[1].trim_matches('"').replace(r#"\\"#, r#"\"#)
            } else {
                return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("Failed to extract Directory from '{}'", res_str)));
            };
            debug!("got mountpoint directory: '{}'", directory);
            
            let res_str = res_map.get_string_property("Volume")?;
            debug!("res_str Volume: '{}'", res_str);
            let parts: Vec<&str> = res_str.split("=").collect();
            let volume = if parts.len() == 2 {
                parts[1] // .trim_matches('"').replace(r#"\\"#, r#"\"#)
            } else {
                return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("Failed to extract Volume from '{}'", res_str)));
            };

            debug!("got mountpoint volume: '{}'", volume);

            result.push(MountPoint {
                directory: PathBuf::from(directory),
                volume: Volume::query_by_device_id(&volume)?
            });
        }

        Ok(result)
    }

    pub fn get_directory(&'a self) -> &'a Path {
        &self.directory
    }

    pub fn get_volume(&'a self) -> &'a Volume {
        &self.volume
    }

}