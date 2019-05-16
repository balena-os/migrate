use failure::ResultExt;
use lazy_static::lazy_static;
use regex::{Captures, Regex};
use serde::Deserialize;
use serde_json;
use std::fs::{read_to_string, File};
use std::path::{Path, PathBuf};

use crate::{
    common::{MigErrCtx, MigError, MigErrorKind},
    defs::KERNEL_CMDLINE_PATH,
    linux_common::{call_cmd, get_root_info, path_info::PathInfo, FDISK_CMD, LSBLK_CMD},
};

const GPT_EFI_PART: &str = "C12A7328-F81F-11D2-BA4B-00A0C93EC93B";

#[derive(Debug, Deserialize)]
pub(crate) struct LsblkPartition {
    pub name: String,
    pub kname: String,
    pub uuid: Option<String>,
    pub fstype: Option<String>,
    pub mountpoint: Option<String>,
    pub label: Option<String>,
    pub parttype: Option<String>,
    pub partlabel: Option<String>,
    pub partuuid: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LsblkDevice {
    pub name: String,
    pub kname: String,
    pub uuid: Option<String>,
    children: Vec<LsblkPartition>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct LsblkInfo {
    blockdevices: Vec<LsblkDevice>,
}

impl LsblkInfo {
    pub fn new() -> Result<&'static LsblkInfo, MigError> {
        lazy_static! {
            static ref LSBLK_INFO: LsblkInfo = {
                let args: Vec<&str> = vec!["-b", "-O", "--json"];
                let cmd_res = call_cmd(LSBLK_CMD, &args, true)?;
                if cmd_res.status.success() {
                    serde_json::from_str(&cmd_res.stdout).context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        "failed to deserialze lsblk output from json",
                    ))?
                } else {
                    Err(MigError::from_remark(
                        MigErrorKind::ExecProcess,
                        "new: failed to determine block device attributes for",
                    ))
                }
            };
        }

        Ok(&LSBLK_INFO)
    }

    pub fn get_devinfo_from_partition(
        &self,
        part_path: &Path,
    ) -> Result<(&'static LsblkDevice, &'static LsblkPartition), MigError> {
        if let Some(part_name) = part_path.file_name() {
            let cmp_name = part_name.to_string_lossy();
            // TODO: use map instead of find to keep on searching ?
            if let Some(lsblk_dev) = self
                .blockdevices
                .iter()
                .find(|&dev| *&cmp_name.starts_with(&dev.name))
            {
                if let Some(part_info) = lsblk_dev
                    .children
                    .iter()
                    .find(|&part| part.name == cmp_name.as_ref())
                {
                    Ok((lsblk_dev, part_info))
                } else {
                    Err(MigError::from_remark(
                        MigErrorKind::NotFound,
                        &format!(
                            "The partition was not found in lsblk output '{}'",
                            part_path.display()
                        ),
                    ))
                }
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "The device was not found in lsblk output '{}'",
                        part_path.display()
                    ),
                ))
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("The device path is not valid '{}'", part_path.display()),
            ))
        }
    }
}
