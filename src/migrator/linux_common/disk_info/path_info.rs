use failure::ResultExt;
use lazy_static::lazy_static;
use log::{debug, trace};
use regex::Regex;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use crate::linux_common::disk_info::lsblk_info::{LsblkDevice, LsblkPartition};
use crate::{
    common::{dir_exists, format_size_with_unit, MigErrCtx, MigError, MigErrorKind},
    defs::ROOT_PATH,
    linux_common::{call_cmd, disk_info::lsblk_info::LsblkInfo, get_root_info, DF_CMD},
};

const MODULE: &str = "linux_common::path_info";

const SIZE_REGEX: &str = r#"^(\d+)K?$"#;

const MOUNT_REGEX: &str = r#"^(\S+)\s+on\s+(\S+)\s+type\s+(\S+)\s+\(([^\)]+)\).*$"#;

#[derive(Debug, Clone)]
pub(crate) struct PathInfo {
    // the absolute path
    pub path: PathBuf,
    // the partition device path
    pub drive: PathBuf,
    // the partition fs type
    pub drive_size: u64,
    // the partition fs type
    pub device: PathBuf,
    // the partition index
    pub index: u16,
    // the devices mountpoint
    pub mountpoint: PathBuf,
    // the drive device path
    pub fs_type: String,
    // the partition read only flag
    pub mount_ro: bool,
    // the partition uuid
    pub uuid: Option<String>,
    // the partition partuuid
    pub part_uuid: Option<String>,
    // the partition label
    pub part_label: Option<String>,
    // the partition size
    pub part_size: u64,
    // the fs size
    pub fs_size: u64,
    // the fs free space
    pub fs_free: u64,
}

impl PathInfo {
    pub fn from_mounted<P: AsRef<Path>>(
        abs_path: &P,
        device: &LsblkDevice,
        partition: &LsblkPartition,
    ) -> Result<PathInfo, MigError> {
        let abs_path = abs_path.as_ref().to_path_buf();

        trace!("from_mounted: entered with: path: '{}', device: '{}', partition: '{}'", abs_path.display(), device.name, partition.name);

        debug!("looking fo path: '{}'", abs_path.display());

        lazy_static! {
            static ref SIZE_RE: Regex = Regex::new(SIZE_REGEX).unwrap();
            static ref MOUNT_RE: Regex = Regex::new(MOUNT_REGEX).unwrap();
        }

        let res_path = abs_path.to_string_lossy();

        let args: Vec<&str> = vec!["--block-size=K", "--output=size,used", &res_path];

        let cmd_res = call_cmd(DF_CMD, &args, true)?;

        if !cmd_res.status.success() || cmd_res.stdout.is_empty() {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::new: failed to find mountpoint for {}",
                    MODULE,
                    abs_path.display()
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
                    abs_path.display()
                ),
            ));
        }

        // debug!("PathInfo::new: '{}' df result: {:?}", path, &output[1]);

        let words: Vec<&str> = output[1].split_whitespace().collect();
        if words.len() != 2 {
            debug!(
                "PathInfo::new: '{}' df result: words {}",
                abs_path.display(),
                words.len()
            );
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::new: failed to parse mountpoint attributes for {}",
                    MODULE,
                    abs_path.display()
                ),
            ));
        }

        debug!(
            "PathInfo::new: '{}' df result: {:?}",
            abs_path.display(),
            &words
        );

        let fs_size = if let Some(captures) = SIZE_RE.captures(words[0]) {
            captures
                .get(1)
                .unwrap()
                .as_str()
                .parse::<u64>()
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("{}::new: failed to parse size from {} ", MODULE, words[0]),
                ))?
                * 1024
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("{}::new: failed to parse size from {} ", MODULE, words[0]),
            ));
        };

        let fs_used = if let Some(captures) = SIZE_RE.captures(words[1]) {
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

        let fs_free = fs_size - fs_used;

        let result = PathInfo {
            path: abs_path,
            device: PathBuf::from(partition.get_path()),
            index: if let Some(index) = partition.index {
                index
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "index not found for partition: '{}'",
                        partition.get_path().display()
                    ),
                ));
            },
            mountpoint: if let Some(ref mountpoint) = partition.mountpoint {
                PathBuf::from(mountpoint)
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "mountpoint not found for partition: '{}'",
                        partition.get_path().display()
                    ),
                ));
            },
            drive: PathBuf::from(device.get_path()),
            drive_size: if let Some(ref size) = device.size {
                size.parse::<u64>().context(MigErrCtx::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "Could not parse drive size for partition: '{}'",
                        partition.get_path().display()
                    ),
                ))?
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "drive size not found for partition: '{}'",
                        partition.get_path().display()
                    ),
                ));
            },
            fs_type: if let Some(ref fs_type) = partition.fstype {
                fs_type.clone()
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "fs_type not found for partition: '{}'",
                        partition.get_path().display()
                    ),
                ));
            },
            mount_ro: partition.ro == "1",
            uuid: partition.uuid.clone(),
            part_uuid: partition.partuuid.clone(),
            part_label: partition.partlabel.clone(),
            part_size: if let Some(ref size) = partition.size {
                size.parse::<u64>().context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "size could not be parsed for partition: '{}'",
                        partition.get_path().display()
                    ),
                ))?
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "size not found for partition: '{}'",
                        partition.get_path().display()
                    ),
                ));
            },
            fs_size,
            fs_free,
        };

        debug!(
            "PathInfo::new: '{}' lsblk result: '{:?}'",
            result.path.display(),
            result
        );

        Ok(result)
    }

    pub fn new<P: AsRef<Path>>(
        path: P,
        lsblk_info: &LsblkInfo,
    ) -> Result<Option<PathInfo>, MigError> {
        let path = path.as_ref();

        trace!("PathInfo::new: entered with: '{}'", path.display());

        if !dir_exists(path)? {
            return Ok(None);
        }

        let abs_path = std::fs::canonicalize(path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "{}::new: failed to create absolute path from {}",
                MODULE,
                path.display()
            ),
        ))?;

        debug!("looking fo path: '{}'", abs_path.display());

        let (device, partition) = if abs_path == Path::new(ROOT_PATH) {
            let (root_device, _root_fs_type) = get_root_info()?;
            lsblk_info.get_devinfo_from_partition(root_device)?
        } else {
            lsblk_info.get_path_info(&abs_path)?
        };

        Ok(Some(PathInfo::from_mounted(
            &abs_path, &device, &partition,
        )?))
    }
}

impl Display for PathInfo {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "path: {} device: {}, uuid: {}, fstype: {}, size: {}, fs_size: {}, fs_free: {}",
            self.path.display(),
            self.device.display(),
            if let Some(ref uuid) = self.uuid {
                uuid.as_str()
            } else {
                "-"
            },
            self.fs_type,
            format_size_with_unit(self.part_size),
            format_size_with_unit(self.fs_size),
            format_size_with_unit(self.fs_free)
        )
    }
}
