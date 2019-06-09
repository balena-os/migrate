use log::debug;
use std::path::{PathBuf};

// use log::{debug};
use super::QueryRes;
use crate::{
    defs::{FileSystem},
    common::{MigError, MigErrorKind},
    mswin::{
        powershell::PSInfo,
        win_api::wmi_api::WmiAPI,
        wmi_utils::volume::Volume,
    },
};

use regex::Regex;



const QUERY_ALL: &str = "SELECT Directory,Volume FROM Win32_MountPoint";

pub(crate) struct MountPoint {
    directory: PathBuf,
    volume: Volume
}


impl PathBuf {
    pub fn query_all() -> Result<Vec<MountPoint>, MigError> {
        let q_res = WmiAPI::get_api(NS_CVIM2)?.raw_query(QUERY_ALL)?;
        let mut result: Vec<MountPoint> = Vec::new();
        for res in q_res {
// expected
//  Directory        : Win32_Directory.Name="C:\\"
//  Volume           : Win32_Volume.DeviceID="\\\\?\\Volume{927a901b-d6fe-4133-a909-11b2ec00d54a}\\"

            let res_map = QueryRes::new(&res);

            let directory = res_map.get_string_property("Directory")?;
            debug!("Directory: '{}'", directory);
            let parts = directory.split("=");
            if parts.len() == 2 {
                let dir = parts[1].trim_matches('"').replace(r#"\\"#, r#"\"#);
                debug!("Dir: '{}'", dir);
            }

            // result.push(MountPoint::new(res_map)?);
        }
        Ok(result)
    }

}