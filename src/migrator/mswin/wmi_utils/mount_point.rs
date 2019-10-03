use log::debug;
use std::path::{Path, PathBuf};

// use log::{debug};
use super::QueryRes;
use crate::{
    common::{MigError, MigErrorKind},
    mswin::{win_api::wmi_api::WmiAPI, wmi_utils::volume::Volume},
};

use crate::mswin::wmi_utils::NS_CVIM2;

const QUERY_ALL: &str = "SELECT Directory,Volume FROM Win32_MountPoint";

const QUERY_VOL2DIR: &str =
    r#"SELECT Directory FROM Win32_MountPoint where Volume='Win32_Volume.DeviceID=""'"#;

const QUERY_DIR2VOL: &str =
    r#"SELECT Volume FROM Win32_MountPoint where Directory'Win32_Volume.DeviceID=""'"#;

#[derive(Clone)]
pub(crate) struct MountPoint {
    directory: PathBuf,
    volume: Volume,
}

impl<'a> MountPoint {
    pub fn query_all() -> Result<Vec<MountPoint>, MigError> {
        Ok(MountPoint::from_query(QUERY_ALL)?)
    }

    pub fn query_path<P: AsRef<Path>>(path: P) -> Result<MountPoint, MigError> {
        let path = path.as_ref();
        let mountpoints = MountPoint::from_query(QUERY_ALL)?;
        let mut found_mountpoint: Option<MountPoint> = None;

        for ref mountpoint in mountpoints {
            if mountpoint.get_directory().starts_with(path) {
                if let Some(ref found) = found_mountpoint {
                    if mountpoint.directory.to_string_lossy().len()
                        > found.directory.to_string_lossy().len()
                    {
                        found_mountpoint = Some(mountpoint.clone());
                    }
                } else {
                    found_mountpoint = Some(mountpoint.clone());
                }
            }
        }

        // TODO: take precautions for EFI path ?

        if let Some(found_path) = found_mountpoint {
            //got a mount
            debug!(
                "Found mountpoint for path: '{}', Mountpoint: '{}', volume: '{}'",
                path.display(),
                found_path.get_directory().display(),
                found_path.get_volume().get_device_id()
            );

            Ok(found_path)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("No mount found for path: '{}'", path.display()),
            ))
        }
    }

    pub fn query_directory_by_volume(volume: &Volume) -> Result<Option<PathBuf>, MigError> {
        let vol_id = volume.get_device_id();
        for mount_point in MountPoint::query_all()? {
            if mount_point.volume.get_device_id() == vol_id {
                return Ok(Some(mount_point.directory));
            }
        }
        Ok(None)
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
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!("Failed to extract Directory from '{}'", res_str),
                ));
            };
            debug!("got mountpoint directory: '{}'", directory);

            let res_str = res_map.get_string_property("Volume")?;
            debug!("res_str Volume: '{}'", res_str);
            let parts: Vec<&str> = res_str.split("=").collect();
            let volume = if parts.len() == 2 {
                parts[1] // .trim_matches('"').replace(r#"\\"#, r#"\"#)
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!("Failed to extract Volume from '{}'", res_str),
                ));
            };

            debug!("got mountpoint volume: '{}'", volume);

            result.push(MountPoint {
                directory: PathBuf::from(directory),
                volume: Volume::query_by_device_id(&volume)?,
            });
        }

        Ok(result)
    }

    pub fn is_directory(&self, directory: &Path) -> bool {
        directory == self.directory
    }

    pub fn get_directory(&'a self) -> &'a Path {
        &self.directory
    }

    pub fn get_volume(&'a self) -> &'a Volume {
        &self.volume
    }
}
