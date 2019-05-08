use failure::ResultExt;
use lazy_static::lazy_static;
use log::{debug, trace, warn};
use regex::Regex;
use serde_json::Value;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use crate::common::{dir_exists, format_size_with_unit, MigErrCtx, MigError, MigErrorKind};

use super::{call_cmd, DF_CMD, LSBLK_CMD, MOUNT_CMD};

const MODULE: &str = "linux_common::path_info";

const SIZE_REGEX: &str = r#"^(\d+)K?$"#;
const LSBLK_REGEX: &str = r#"^(\S+)\s+(\d+)\s+(\S+)\s+(\S+)(\s+(.*))?$"#;

const MOUNT_REGEX: &str = r#"^(\S+)\s+on\s+(\S+)\s+type\s+(\S+)\s+\(([^\)]+)\).*$"#;

#[derive(Debug)]
pub(crate) struct PathInfo {
    pub path: PathBuf,
    pub device: PathBuf,
    pub drive: PathBuf,
    pub fs_type: String,
    pub mount_ro: bool,
    pub uuid: String,
    pub part_uuid: String,
    pub part_label: String,
    pub part_size: u64,
    pub fs_size: u64,
    pub fs_free: u64,
}

impl PathInfo {
    fn default<P: AsRef<Path>>(path: P) -> PathInfo {
        PathInfo {
            path: PathBuf::from(path.as_ref()),
            device: PathBuf::from(""),
            drive: PathBuf::from(""),
            fs_type: String::from(""),
            mount_ro: false,
            uuid: String::from(""),
            part_uuid: String::from(""),
            part_label: String::from(""),
            part_size: 0,
            fs_size: 0,
            fs_free: 0,
        }
    }

    pub fn new<P: AsRef<Path>>(path: P) -> Result<Option<PathInfo>, MigError> {
        let path = path.as_ref();
        trace!("PathInfo::new: entered with: '{}'", path.display());

        if !dir_exists(path)? {
            return Ok(None);
        }

        let abs_path = std::fs::canonicalize(Path::new(path)).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "{}::new: failed to create absolute path from {}",
                MODULE, path.display()
            ),
        ))?;

        lazy_static! {
            static ref LSBLK_RE: Regex = Regex::new(LSBLK_REGEX).unwrap();
            static ref SIZE_RE: Regex = Regex::new(SIZE_REGEX).unwrap();
            static ref MOUNT_RE: Regex = Regex::new(MOUNT_REGEX).unwrap();
        }

        let mut result = PathInfo::default(&abs_path);
        let res_path = result.path.to_string_lossy();

        let args: Vec<&str> = vec!["--block-size=K", "--output=source,size,used", &res_path];

        let cmd_res = call_cmd(DF_CMD, &args, true)?;

        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::new: failed to find mountpoint for {}",
                    MODULE,
                    result.path.display()
                ),
            ));
        }

        let output: Vec<&str> = cmd_res.stdout.lines().collect();
        if output.len() != 2 {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::new: failed to parse mountpoint attributes for {}",
                    MODULE,
                    result.path.display()
                ),
            ));
        }

        // debug!("PathInfo::new: '{}' df result: {:?}", path, &output[1]);

        let words: Vec<&str> = output[1].split_whitespace().collect();
        if words.len() != 3 {
            debug!(
                "PathInfo::new: '{}' df result: words {}",
                result.path.display(),
                words.len()
            );
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::new: failed to parse mountpoint attributes for {}",
                    MODULE,
                    result.path.display()
                ),
            ));
        }

        debug!("PathInfo::new: '{}' df result: {:?}", path.display(), &words);

        if words[0] == "/dev/root" {
            let args: Vec<&str> = vec![];
            let cmd_res = call_cmd(MOUNT_CMD, &args, true)?;
            if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::new: failed to find mountpoint for {}",
                        MODULE,
                        result.path.display()
                    ),
                ));
            }

            let mut found = false;
            for mount in cmd_res.stdout.lines() {
                debug!("looking at '{}'", mount);
                if let Some(captures) = MOUNT_RE.captures(mount) {
                    if captures.get(2).unwrap().as_str() == "/" {
                        result.device = PathBuf::from(captures.get(1).unwrap().as_str());
                        found = true;
                        break;
                    }
                } else {
                    warn!("unable to parse mount '{}'", mount);
                }
            }
            if !found {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::new: failed to find device in mounts: '{}' ",
                        MODULE, words[0]
                    ),
                ));
            }
        } else {
            result.device = PathBuf::from(words[0]);
        }

        result.fs_size = if let Some(captures) = SIZE_RE.captures(words[1]) {
            captures
                .get(1)
                .unwrap()
                .as_str()
                .parse::<u64>()
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("{}::new: failed to parse size from {} ", MODULE, words[1]),
                ))?
                * 1024
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("{}::new: failed to parse size from {} ", MODULE, words[1]),
            ));
        };

        let fs_used = if let Some(captures) = SIZE_RE.captures(words[2]) {
            captures
                .get(1)
                .unwrap()
                .as_str()
                .parse::<u64>()
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("{}::new: failed to parse size from {} ", MODULE, words[2]),
                ))?
                * 1024
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("{}::new: failed to parse size from {} ", MODULE, words[2]),
            ));
        };

        result.fs_free = result.fs_size - fs_used;

        let args: Vec<&str> = vec!["-b", "-O", "--json"];

        let cmd_res = call_cmd(LSBLK_CMD, &args, true)?;
        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &format!(
                    "{}::new: failed to determine block device attributes for '{}'",
                    MODULE,
                    result.path.display()
                ),
            ));
        }

        let parse_res: Value =
            serde_json::from_str(&cmd_res.stdout).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "{}::new: failed to parse lsblk json output: '{}'",
                    MODULE, &cmd_res.stdout
                ),
            ))?;

        if let Some(dev_name) = Path::new(&result.device).file_name() {
            let dev_name = dev_name.to_str().unwrap();
            if let Value::Array(devs) = parse_res.get("blockdevices").unwrap() {
                // iterate over lock devices
                debug!("device: '{}' got devices", dev_name);
                for device in devs {
                    if let Value::String(ref name) = device.get("name").unwrap() {
                        trace!("device: '{}' looking at '{}'", dev_name, name);
                        if dev_name.starts_with(name) {
                            // found our block device
                            debug!("device: {} found {}", dev_name, name);
                            result.drive = PathBuf::from(&format!("/dev/{}", name));
                            if let Value::Array(children) = device.get("children").unwrap() {
                                // iterate over children -> partitions
                                for child_dev in children {
                                    if let Value::String(name) = child_dev.get("name").unwrap() {
                                        if name == &dev_name {
                                            // found our partition
                                            debug!("device: {} found {}", dev_name, name);
                                            if let Some(ref val) = child_dev.get("size") {
                                                if let Value::String(ref s) = val {
                                                    result.part_size = s.parse::<u64>().context(MigErrCtx::from_remark(
                                                        MigErrorKind::Upstream,
                                                        &format!("{}::new: failed to parse size from {}", MODULE, s),
                                                    ))?;
                                                }
                                            }

                                            if let Some(ref val) = child_dev.get("fstype") {
                                                if let Value::String(ref s) = val {
                                                    result.fs_type = String::from(s.as_ref());
                                                }
                                            }

                                            if let Some(ref val) = child_dev.get("ro") {
                                                if let Value::String(ref s) = val {
                                                    result.mount_ro = s == "1";
                                                }
                                            }

                                            if let Some(ref val) = child_dev.get("fstype") {
                                                if let Value::String(ref s) = val {
                                                    result.fs_type = String::from(s.as_ref());
                                                }
                                            }

                                            if let Some(ref val) = child_dev.get("uuid") {
                                                if let Value::String(ref s) = val {
                                                    result.uuid = String::from(s.as_ref());
                                                }
                                            }

                                            if let Some(ref val) = child_dev.get("partuuid") {
                                                if let Value::String(ref s) = val {
                                                    result.part_uuid = String::from(s.as_ref());
                                                }
                                            }

                                            if let Some(ref val) = child_dev.get("partlabel") {
                                                if let Value::String(ref s) = val {
                                                    result.part_label = String::from(s.as_ref());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::new: failed to parse block device attributes for {} from lsblk output",
                    MODULE,
                    result.path.display()
                ),
            ));
        }

        debug!(
            "PathInfo::new: '{}' lsblk result: '{:?}'",
            result.path.display(),
            result
        );
        if result.fs_type.is_empty() || result.part_size == 0 {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::new: failed to parse block device attributes for {} from lsblk output",
                    MODULE,
                    result.path.display()
                ),
            ));
        }

        Ok(Some(result))
    }
}

impl Display for PathInfo {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "path: {} device: {}, uuid: {}, fstype: {}, size: {}, fs_size: {}, fs_free: {}",
            self.path.display(),
            self.device.display(),
            self.uuid,
            self.fs_type,
            format_size_with_unit(self.part_size),
            format_size_with_unit(self.fs_size),
            format_size_with_unit(self.fs_free)
        )
    }
}
