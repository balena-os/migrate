use failure::ResultExt;
use lazy_static::lazy_static;
use log::{debug, trace, warn};
use regex::Regex;
use std::path::{Path, PathBuf};

use crate::linux::linux_common::to_std_device_path;
use crate::linux::linux_defs::UDEVADM_CMD;
use crate::{
    common::{call, file_exists, path_append, MigErrCtx, MigError, MigErrorKind},
    defs::{DISK_BY_LABEL_PATH, DISK_BY_PARTUUID_PATH, DISK_BY_UUID_PATH},
    linux::linux_defs::LSBLK_CMD,
};
use std::collections::HashMap;

pub(crate) mod partition;
use partition::Partition;

pub(crate) mod device;
use partition::Device;

// const GPT_EFI_PART: &str = "C12A7328-F81F-11D2-BA4B-00A0C93EC93B";

const BLOC_DEV_SUPP_MAJ_NUMBERS: [&str; 45] = [
    "3", "8", "9", "21", "33", "34", "44", "48", "49", "50", "51", "52", "53", "54", "55", "56",
    "57", "58", "64", "65", "66", "67", "68", "69", "70", "71", "72", "73", "74", "75", "76", "77",
    "78", "79", "80", "81", "82", "83", "84", "85", "86", "87", "179", "180", "259",
];

enum ResultType {
    Drive(LsblkDevice),
    Partition(LsblkPartition),
}

fn call_lsblk<P: AsRef<Path>>(
    device: Option<&P>,
) -> Result<Vec<HashMap<String, String>>, MigError> {
    const LSBLK_COLS: &str = "NAME,KNAME,MAJ:MIN,FSTYPE,MOUNTPOINT,LABEL,UUID,SIZE,RO,TYPE";
    let mut args = vec!["-b", "-P", "-o", LSBLK_COLS];
    if let Some(device) = device {
        args.push(&*device.as_ref().to_string_lossy())
    }

    let lsblk_cmd_res = call(LSBLK_CMD, &args, true)?;
    if lsblk_cmd_res.status.success() {
        let mut lsblk_results: Vec<HashMap<String, String>> = Vec::new();
        for line in lsblk_cmd_res.stdout.lines().skip(1) {
            lsblk_results.push(parse_lsblk_line(line)?);
        }
        Ok(lsblk_results)
    } else {
        Err(MigError::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "call_lsblk: lsblk failed with message: '{}'",
                lsblk_cmd_res.stderr
            ),
        ))
    }
}

fn call_udevadm<P: AsRef<Path>>(device: P) -> Result<HashMap<String, String>, MigError> {
    let udev_cmd_res = call(
        UDEVADM_CMD,
        &[
            "info",
            "-q",
            "property",
            &*device.as_ref().to_string_lossy(),
        ],
        true,
    )?;
    if udev_cmd_res.status.success() {
        lazy_static! {
            static ref UDEV_PARAM_RE: Regex = Regex::new(r##"^([^=]+)=(.*)$"##).unwrap();
        }

        let mut udev_result: HashMap<String, String> = HashMap::new();
        for line in udev_cmd_res.stdout.lines() {
            if let Some(captures) = UDEV_PARAM_RE.captures(line) {
                udev_result.insert(
                    String::from(captures.get(1).unwrap().as_str()),
                    String::from(captures.get(2).unwrap().as_str()),
                );
            } else {
                warn!("Failed to parse udevadm output: '{}'", line);
                return Err(MigError::displayed());
            }
        }

        Ok(udev_result)
    } else {
        Err(MigError::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "call_udevadm: udevadm failed with message: '{}'",
                udev_cmd_res.stderr
            ),
        ))
    }
}

fn parse_lsblk_line(line: &str) -> Result<HashMap<String, String>, MigError> {
    debug!("parse_line: called with '{}'", line);

    lazy_static! {
        static ref LSBLK_PARAM_RE: Regex =
            Regex::new(r##"^([\S^=]+)="([^"]*)"(\s+(.*))?$"##).unwrap();
    }

    trace!("from_list: processing line: '{}'", line);
    let mut curr_pos = line;
    let mut result: HashMap<String, String> = HashMap::new();

    // parse current line into hashmap
    loop {
        debug!("parsing '{}'", curr_pos);
        if let Some(captures) = LSBLK_PARAM_RE.captures(curr_pos) {
            let param_name = captures.get(1).unwrap().as_str();
            let param_value = captures.get(2).unwrap().as_str();

            if !param_value.is_empty() {
                result.insert(String::from(param_name), String::from(param_value));
            }

            if let Some(ref rest) = captures.get(4) {
                curr_pos = rest.as_str();
                debug!(
                    "Found param: '{}', value '{}', rest '{}'",
                    param_name, param_value, curr_pos
                );
            } else {
                debug!(
                    "Found param: '{}', value '{}', rest None",
                    param_name, param_value
                );
                break;
            }
        } else {
            warn!("Failed to parse lsblk output '{}'", curr_pos);
            return Err(MigError::displayed());
        }
    }

    Ok(result)
}

#[derive(Debug, Clone)]
pub(crate) struct LsblkInfo {
    blockdevices: Vec<LsblkDevice>,
}

impl<'a> LsblkInfo {
    pub fn lsblk_partition_from_dev_path<P: AsRef<Path>>(
        path: P,
    ) -> Result<LsblkPartition, MigError> {
        debug!("lsblk_partition_from_dev_path: {}", path.as_ref().display());
        Ok(LsblkInfo::call_lsblk_for_partition(path)?)
    }

    pub fn all() -> Result<LsblkInfo, MigError> {
        debug!("all:");
        LsblkInfo::from_string(&LsblkInfo::call_lsblk_for_all()?)
    }

    pub fn get_blk_devices(&'a self) -> &'a Vec<LsblkDevice> {
        &self.blockdevices
    }

    pub fn get_devices_for_partuuid(
        &'a self,
        partuuid: &str,
    ) -> Result<(&'a LsblkDevice, &'a LsblkPartition), MigError> {
        debug!("get_devices_for_partuuid: {}", partuuid);
        for device in &self.blockdevices {
            if let Some(ref children) = device.children {
                if let Some(partition) = children.iter().find(|part| {
                    if let Some(ref curr_uuid) = part.partuuid {
                        curr_uuid.as_str() == partuuid
                    } else {
                        false
                    }
                }) {
                    return Ok((device, partition));
                }
            }
        }
        Err(MigError::from_remark(
            MigErrorKind::NotFound,
            &format!("No partition found for partuuid: '{}'", partuuid),
        ))
    }

    pub fn get_devices_for_uuid(
        &'a self,
        uuid: &str,
    ) -> Result<(&'a LsblkDevice, &'a LsblkPartition), MigError> {
        debug!("get_devices_for_uuid: {}", uuid);

        for device in &self.blockdevices {
            if let Some(ref children) = device.children {
                if let Some(partition) = children.iter().find(|part| {
                    if let Some(ref curr_uuid) = part.uuid {
                        curr_uuid.as_str() == uuid
                    } else {
                        false
                    }
                }) {
                    return Ok((device, partition));
                }
            }
        }
        Err(MigError::from_remark(
            MigErrorKind::NotFound,
            &format!("No partition found for uuid: '{}'", uuid),
        ))
    }

    pub fn get_devices_for_label(
        &'a self,
        label: &str,
    ) -> Result<(&'a LsblkDevice, &'a LsblkPartition), MigError> {
        debug!("get_devices_for_label: {}", label);
        for device in &self.blockdevices {
            if let Some(ref children) = device.children {
                if let Some(partition) = children.iter().find(|part| {
                    if let Some(ref curr_label) = part.partlabel {
                        curr_label.as_str() == label
                    } else {
                        false
                    }
                }) {
                    return Ok((device, partition));
                }
            }
        }
        Err(MigError::from_remark(
            MigErrorKind::NotFound,
            &format!("No partition found for label: '{}'", label),
        ))
    }

    pub fn get_devices_for_path<P: AsRef<Path>>(
        &'a self,
        path: P,
    ) -> Result<(&'a LsblkDevice, &'a LsblkPartition), MigError> {
        let path = path.as_ref();
        debug!("get_devices_for_path: '{}", path.display());
        let abs_path = path.canonicalize().context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("failed to canonicalize path: '{}'", path.display()),
        ))?;

        let mut mp_match: Option<(&LsblkDevice, &LsblkPartition)> = None;

        for device in &self.blockdevices {
            debug!(
                "get_path_info: looking at device '{}",
                device.get_path().display()
            );
            if let Some(ref children) = device.children {
                for part in children {
                    debug!(
                        "get_path_info: looking at partition '{}",
                        part.get_path().display()
                    );
                    if let Some(ref mountpoint) = part.mountpoint {
                        if abs_path == PathBuf::from(mountpoint) {
                            debug!(
                                "get_path_info: looking at partition found equal at '{}'",
                                mountpoint.display()
                            );
                            return Ok((&device, part));
                        } else if abs_path.starts_with(mountpoint) {
                            if let Some((_last_dev, last_part)) = mp_match {
                                if last_part
                                    .mountpoint
                                    .as_ref()
                                    .unwrap()
                                    .to_string_lossy()
                                    .len()
                                    > mountpoint.to_string_lossy().len()
                                {
                                    mp_match = Some((&device, part))
                                }
                            } else {
                                mp_match = Some((&device, part))
                            }
                        }
                    }
                }
            }
        }

        if let Some(res) = mp_match {
            Ok(res)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!(
                    "A mountpoint could not be found for path: '{}'",
                    path.display()
                ),
            ))
        }
    }

    // get the LsblkDevice & LsblkPartition from partition device path as in /dev/sda1
    pub fn get_devices_for_partition<P: AsRef<Path>>(
        &'a self,
        part_path: P,
    ) -> Result<(&'a LsblkDevice, &'a LsblkPartition), MigError> {
        let part_path = part_path.as_ref();
        debug!("get_devices_for_partition: '{}", part_path.display());

        let part_path = to_std_device_path(part_path)?;

        if let Some(part_name) = part_path.file_name() {
            let cmp_name = part_name.to_string_lossy();
            if let Some(lsblk_dev) = self.blockdevices.iter().find(|&dev| {
                debug!("cmp: {} == dev: {}", cmp_name, dev.name);
                *&cmp_name.starts_with(&dev.name)
            }) {
                Ok((lsblk_dev, lsblk_dev.get_devinfo_from_part_name(&cmp_name)?))
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

    fn call_lsblk_for_partition<P: AsRef<Path>>(partition: P) -> Result<LsblkPartition, MigError> {
        let lsblk_results = LsblkInfo::call_lsblk(Some(partition))?;
        // expect just one result of type partition
        if lsblk_results.len() == 1 {
            let udev_result = LsblkInfo::call_udevadm(partition)?;
            if let Some(dev_type) = udev_result.get("DEVTYPE") {
                match dev_type.as_str() {
                    "partition" => {
                        Ok(LsblkPartition::new(&lsblk_results[0], &udev_result)?)
                    },
                    _ => Err(MigError::from_remark(MigErrorKind::InvParam,
                                                   &format!("call_lsblk_for_part: invalid device type, expected partition, got: '{}'", dev_type))),
                }
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    "call_lsblk_for_part: Failed to retrieved udevadm parameter DEVTYPE",
                ))
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "call_lsblk_for_part: Invalid number of lsblk results encountered: {}",
                    lsblk_results.len()
                ),
            ))
        }
    }

    fn call_lsblk_for_device<P: AsRef<Path>>(device: P) -> Result<LsblkDevice, MigError> {
        let mut lsblk_results = LsblkInfo::call_lsblk(Some(device))?;
        if let Some(lsblk_result) = lsblk_results.pop() {
            let mut lsblk_device: LsblkDevice =
                LsblkDevice::new(&lsblk_result, &LsblkInfo::call_udevadm(&device)?)?;
            // add partitions
            for lsblk_result in lsblk_results {
                if let Some(dev_name) = lsblk_result.get("NAME") {
                    let udev_result = LsblkInfo::call_udevadm(&dev_name)?;
                    if let Some(dev_type) = udev_result.get("DEVTYPE") {
                        match dev_type.as_str() {
                            "partition" => {
                                let partition = LsblkPartition::new(&lsblk_result, &udev_result)?;
                                    if let Some(ref mut children) = lsblk_device.children {
                                        children.push(partition)
                                    } else {
                                        let mut children: Vec<LsblkPartition> = Vec::new();
                                        children.push(partition);
                                        lsblk_device.children = Some(children)
                                    }
                            },
                            _ => Err(MigError::from_remark(MigErrorKind::InvParam,
                                                           &format!("call_lsblk_for_part: invalid device type, expected partition, got: '{}'", dev_type))),
                        }
                    }
                } else {
                    Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        "call_lsblk_for_part: Failed to retrieved udevadm parameter DEVTYPE",
                    ))
                }
            }
            Ok(lsblk_device)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                "call_lsblk_for_device: No result from call_lsblk",
            ))
        }
    }

    fn get_str(params: &HashMap<String, String>, name: &str) -> Result<String, MigError> {
        if let Some(res) = params.get(name) {
            Ok(res.clone())
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("Parameter '{}' not found", name),
            ))
        }
    }

    fn get_u64(params: &HashMap<String, String>, name: &str) -> Result<Option<u64>, MigError> {
        if let Some(res) = params.get(name) {
            Ok(Some(res.parse::<u64>().context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to parse u64 from '{}'", name),
            ))?))
        } else {
            Ok(None)
        }
    }

    fn get_pathbuf_or_none(params: &HashMap<String, String>, name: &str) -> Option<PathBuf> {
        if let Some(res) = params.get(name) {
            if res.is_empty() {
                None
            } else {
                Some(PathBuf::from(res))
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::linux::lsblk_info::LsblkInfo;

    const LSBLK_OUTPUT1: &str = r##"NAME="sda" KNAME="sda" MAJ:MIN="8:0" FSTYPE="" MOUNTPOINT="" LABEL="" UUID="" PARTUUID="" RO="0" SIZE="2000365289472" TYPE="disk"
NAME="sda1" KNAME="sda1" MAJ:MIN="8:1" FSTYPE="ext4" MOUNTPOINT="/run/media/thomas/003bd8b2-bc1d-4fc0-a08b-a72427945ff5" LABEL="" UUID="003bd8b2-bc1d-4fc0-a08b-a72427945ff5" PARTUUID="406be993-ed9b-41eb-8902-1603bd368d88" RO="0" SIZE="2000363192320" TYPE="part"
NAME="nvme0n1" KNAME="nvme0n1" MAJ:MIN="259:0" FSTYPE="" MOUNTPOINT="" LABEL="" UUID="" PARTUUID="" RO="0" SIZE="512110190592" TYPE="disk"
NAME="nvme0n1p1" KNAME="nvme0n1p1" MAJ:MIN="259:1" FSTYPE="vfat" MOUNTPOINT="/boot/efi" LABEL="ESP" UUID="42D3-AAB8" PARTUUID="ea85e980-ee1a-464a-928a-dde13eec7e83" RO="0" SIZE="713031680" TYPE="part"
NAME="nvme0n1p2" KNAME="nvme0n1p2" MAJ:MIN="259:2" FSTYPE="" MOUNTPOINT="" LABEL="" UUID="" PARTUUID="87d21a9d-d97c-44cc-a32f-95f396169174" RO="0" SIZE="134217728" TYPE="part"
NAME="nvme0n1p3" KNAME="nvme0n1p3" MAJ:MIN="259:3" FSTYPE="BitLocker" MOUNTPOINT="" LABEL="" UUID="" PARTUUID="ffd6781b-4f09-4378-a2f8-54aa294eb265" RO="0" SIZE="79322677248" TYPE="part"
NAME="nvme0n1p4" KNAME="nvme0n1p4" MAJ:MIN="259:4" FSTYPE="ntfs" MOUNTPOINT="" LABEL="WINRETOOLS" UUID="500EC0840EC06516" PARTUUID="5646ec29-6cdd-401a-96ce-bbfa62a4b7cb" RO="0" SIZE="1038090240" TYPE="part"
NAME="nvme0n1p5" KNAME="nvme0n1p5" MAJ:MIN="259:5" FSTYPE="ntfs" MOUNTPOINT="" LABEL="Image" UUID="C614C0AC14C0A0B3" PARTUUID="a2ef7db6-6201-45f7-906b-a38da95ca5bd" RO="0" SIZE="10257170432" TYPE="part"
NAME="nvme0n1p6" KNAME="nvme0n1p6" MAJ:MIN="259:6" FSTYPE="ntfs" MOUNTPOINT="" LABEL="DELLSUPPORT" UUID="AA88E9D888E9A2D5" PARTUUID="3c84360b-7732-4344-b39c-f92ca7ef1db3" RO="0" SIZE="1212153856" TYPE="part"
NAME="nvme0n1p7" KNAME="nvme0n1p7" MAJ:MIN="259:7" FSTYPE="ext4" MOUNTPOINT="/mnt/ubuntu" LABEL="" UUID="b305522d-faa7-49fc-a7d1-70dae48bcc3e" PARTUUID="02cf676b-12b6-4510-88e3-804bf71e00f1" RO="0" SIZE="209715200000" TYPE="part"
NAME="nvme0n1p8" KNAME="nvme0n1p8" MAJ:MIN="259:8" FSTYPE="ext4" MOUNTPOINT="/" LABEL="" UUID="f5a69346-5cc1-4d1f-b2d5-b17149fdac09" PARTUUID="f4e91901-1892-44d2-b45f-6ae9f26227f4" RO="0" SIZE="209715200000" TYPE="part"
"##;

    #[test]
    fn read_output_ok1() -> () {
        LsblkInfo::from_string(LSBLK_OUTPUT1).unwrap();
    }

    #[test]
    fn get_partition_by_partuuid() -> () {
        let lsblk_info = LsblkInfo::from_string(LSBLK_OUTPUT1).unwrap();
        let (drive, partition) = lsblk_info
            .get_devices_for_partuuid("02cf676b-12b6-4510-88e3-804bf71e00f1")
            .unwrap();
        assert!(drive.name == "nvme0n1");
        assert!(partition.name == "nvme0n1p7");
    }

    #[test]
    fn get_partition_by_uuid() -> () {
        let lsblk_info = LsblkInfo::from_string(LSBLK_OUTPUT1).unwrap();
        let (drive, partition) = lsblk_info.get_devices_for_uuid("500EC0840EC06516").unwrap();
        assert!(drive.name == "nvme0n1");
        assert!(partition.name == "nvme0n1p4");
    }

    /*
    #[test]
    fn get_partition_by_label() -> () {
        let lsblk_info = LsblkInfo::from_string(LSBLK_OUTPUT1).unwrap();
        let (drive, partition) = lsblk_info.get_devices_for_label("ESP").unwrap();
        assert!(drive.name == "nvme0n1");
        assert!(partition.name == "nvme0n1p1");
    }
    */
}
