use failure::ResultExt;
use lazy_static::lazy_static;
use log::{debug, error, trace, warn};
use regex::Regex;
use std::path::{Path, PathBuf};

use crate::linux::linux_common::to_std_device_path;
use crate::{
    common::{call, file_exists, path_append, MigErrCtx, MigError, MigErrorKind},
    defs::{DISK_BY_LABEL_PATH, DISK_BY_PARTUUID_PATH, DISK_BY_UUID_PATH},
    linux::linux_defs::LSBLK_CMD,
};
use std::collections::HashMap;

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

#[derive(Debug, Clone)]
pub(crate) struct LsblkPartition {
    pub name: String,
    pub kname: String,
    pub maj_min: String,
    pub ro: String,
    pub uuid: Option<String>,
    pub fstype: Option<String>,
    pub mountpoint: Option<PathBuf>,
    pub label: Option<String>,
    pub parttype: Option<String>,
    pub partlabel: Option<String>,
    pub partuuid: Option<String>,
    pub size: Option<u64>,
    pub index: Option<u16>,
}

impl LsblkPartition {
    pub fn get_path(&self) -> PathBuf {
        path_append("/dev", &self.name)
    }

    pub fn get_linux_path(&self) -> Result<PathBuf, MigError> {
        let dev_path = if let Some(ref uuid) = self.uuid {
            path_append(DISK_BY_UUID_PATH, uuid)
        } else {
            if let Some(ref partuuid) = self.partuuid {
                path_append(DISK_BY_PARTUUID_PATH, partuuid)
            } else {
                if let Some(ref label) = self.label {
                    path_append(DISK_BY_LABEL_PATH, label)
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::NotFound,
                        &format!("No unique device path found for device: '{}'", self.name),
                    ));
                }
            }
        };
        if file_exists(&dev_path) {
            Ok(dev_path)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("Could not locate device path: '{}'", dev_path.display()),
            ))
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LsblkDevice {
    pub name: String,
    pub kname: String,
    pub maj_min: String,
    pub uuid: Option<String>,
    pub size: Option<u64>,
    pub children: Option<Vec<LsblkPartition>>,
}

impl<'a> LsblkDevice {
    pub fn get_devinfo_from_part_name(
        &'a self,
        part_name: &str,
    ) -> Result<&'a LsblkPartition, MigError> {
        if let Some(ref children) = self.children {
            if let Some(part_info) = children.iter().find(|&part| part.name == part_name) {
                Ok(part_info)
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "The partition was not found in lsblk output '{}'",
                        part_name
                    ),
                ))
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("The device was not found in lsblk output '{}'", part_name),
            ))
        }
    }

    pub fn get_path(&self) -> PathBuf {
        PathBuf::from(&format!("/dev/{}", self.name))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LsblkInfo {
    blockdevices: Vec<LsblkDevice>,
}

impl<'a> LsblkInfo {
    pub fn lsblk_partition_from_dev_path<P: AsRef<Path>>(
        path: P,
    ) -> Result<LsblkPartition, MigError> {
        let lsblk_str = LsblkInfo::call_lsblk(Some(path.as_ref()))?;
        if let Some(ref line) = lsblk_str.lines().nth(0) {
            if let Some(result) = LsblkInfo::parse_line(line)? {
                if let ResultType::Partition(partition) = result {
                    Ok(partition)
                } else {
                    error!("Invalid response from parse_line, not a partition");
                    Err(MigError::displayed())
                }
            } else {
                error!("Invalid response from parse_line, not a partition");
                Err(MigError::displayed())
            }
        } else {
            error!("No data for parse_line - empty result from call_lsblk");
            Err(MigError::displayed())
        }
    }

    pub fn lsblk_device_from_dev_path<P: AsRef<Path>>(path: P) -> Result<LsblkDevice, MigError> {
        let lsblk_str = LsblkInfo::call_lsblk(Some(path.as_ref()))?;
        let mut lsblk_lines = lsblk_str.lines().enumerate();

        let mut device = if let Some((_index, line)) = lsblk_lines.next() {
            if let Some(result) = LsblkInfo::parse_line(line)? {
                match result {
                    ResultType::Drive(device) => device,
                    _ => {
                        error!("Invalid lsblk type - expected device");
                        return Err(MigError::displayed());
                    }
                }
            } else {
                error!("Invalid lsblk type - expected device");
                return Err(MigError::displayed());
            }
        } else {
            error!("Got empty string from LsblkInfo::call_lsblk");
            return Err(MigError::displayed());
        };

        for (_index, line) in lsblk_lines {
            if let Some(result) = LsblkInfo::parse_line(line)? {
                match result {
                    ResultType::Partition(parttition) => {
                        if let Some(ref mut children) = device.children {
                            children.push(parttition)
                        } else {
                            let mut children: Vec<LsblkPartition> = Vec::new();
                            children.push(parttition);
                            device.children = Some(children);
                        }
                    }
                    _ => {
                        error!("Invalid lsblk type - expected partition");
                        return Err(MigError::displayed());
                    }
                }
            } else {
                warn!("Skipping invalid lsblk type - expected partition");
                return Err(MigError::displayed());
            }
        }

        Ok(device)
    }

    pub fn all() -> Result<LsblkInfo, MigError> {
        LsblkInfo::from_string(&LsblkInfo::call_lsblk(None)?)
    }

    pub fn get_blk_devices(&'a self) -> &'a Vec<LsblkDevice> {
        &self.blockdevices
    }

    pub fn get_devices_for_partuuid(
        &'a self,
        partuuid: &str,
    ) -> Result<(&'a LsblkDevice, &'a LsblkPartition), MigError> {
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

    /*
    pub fn get_devices_for_label(
        &'a self,
        label: &str,
    ) -> Result<(&'a LsblkDevice, &'a LsblkPartition), MigError> {
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
    */

    pub fn get_devices_for_path<P: AsRef<Path>>(
        &'a self,
        path: P,
    ) -> Result<(&'a LsblkDevice, &'a LsblkPartition), MigError> {
        let path = path.as_ref();
        trace!("get_path_info: '{}", path.display());
        let abs_path = path.canonicalize().context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("failed to canonicalize path: '{}'", path.display()),
        ))?;

        let mut mp_match: Option<(&LsblkDevice, &LsblkPartition)> = None;

        for device in &self.blockdevices {
            trace!(
                "get_path_info: looking at device '{}",
                device.get_path().display()
            );
            if let Some(ref children) = device.children {
                for part in children {
                    trace!(
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
        trace!("get_devinfo_from_partition: '{}", part_path.display());

        let part_path = to_std_device_path(part_path)?;

        if let Some(part_name) = part_path.file_name() {
            let cmp_name = part_name.to_string_lossy();
            if let Some(lsblk_dev) = self
                .blockdevices
                .iter()
                .find(|&dev| *&cmp_name.starts_with(&dev.name))
            {
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

    fn call_lsblk(device: Option<&Path>) -> Result<String, MigError> {
        #[allow(unused_assignments)]
        let mut dev_name = String::new();
        let args = if let Some(device) = device {
            dev_name = String::from(&*device.to_string_lossy());
            vec![
                "-b",
                "-P",
                "-o",
                "NAME,KNAME,MAJ:MIN,FSTYPE,MOUNTPOINT,LABEL,UUID,PARTUUID,RO,SIZE,TYPE",
                dev_name.as_str(),
            ]
        } else {
            vec![
                "-b",
                "-P",
                "-o",
                "NAME,KNAME,MAJ:MIN,FSTYPE,MOUNTPOINT,LABEL,UUID,PARTUUID,RO,SIZE,TYPE",
            ]
        };

        let cmd_res = call(LSBLK_CMD, &args, true)?;
        if cmd_res.status.success() {
            Ok(cmd_res.stdout)
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                "new: failed to determine block device attributes for",
            ));
        }
    }

    pub fn from_string(data: &str) -> Result<LsblkInfo, MigError> {
        let mut lsblk_info: LsblkInfo = LsblkInfo {
            blockdevices: Vec::new(),
        };

        for line in data.lines() {
            trace!("from_list: processing line: '{}'", line);
            // parse current line into hashmap
            if let Some(result) = LsblkInfo::parse_line(line)? {
                match result {
                    ResultType::Drive(device) => {
                        lsblk_info.blockdevices.push(device);
                    }
                    ResultType::Partition(partition) => {
                        if let Some(device) = lsblk_info.blockdevices.last_mut() {
                            if let Some(children) = device.children.as_mut() {
                                children.push(partition)
                            } else {
                                let mut children: Vec<LsblkPartition> = Vec::new();
                                children.push(partition);
                                device.children = Some(children);
                            }
                        } else {
                            error!(
                                "Invalid sequence while parsing - no device for partition {}",
                                partition.name
                            );
                            return Err(MigError::displayed());
                        }
                    }
                }
            }
        }

        if lsblk_info.blockdevices.is_empty() {
            error!("No devices found");
            return Err(MigError::displayed());
        }

        // filter by maj block device numbers from https://www.kernel.org/doc/Documentation/admin-guide/devices.txt
        // other candidates:
        // 31 block	ROM/flash memory card
        // 45 block	Parallel port IDE disk devices
        // TODO: add more

        let maj_min_re = Regex::new(r#"^(\d+):\d+$"#).unwrap();

        lsblk_info.blockdevices.retain(|dev| {
            if let Some(captures) = maj_min_re.captures(&dev.maj_min) {
                let dev_maj = captures.get(1).unwrap().as_str();
                if let Some(_pos) = BLOC_DEV_SUPP_MAJ_NUMBERS
                    .iter()
                    .position(|&maj| maj == dev_maj)
                {
                    true
                } else {
                    debug!(
                        "rejecting device '{}', maj:min: '{}'",
                        dev.name, dev.maj_min
                    );
                    false
                }
            } else {
                warn!(
                    "Unable to parse device major/minor number from '{}'",
                    dev.maj_min
                );
                false
            }
        });

        debug!("lsblk_info: {:?}", lsblk_info);
        Ok(lsblk_info)
    }

    fn parse_line(line: &str) -> Result<Option<ResultType>, MigError> {
        trace!("parse_line called with '{}'", line);

        lazy_static! {
            static ref PARAM_RE: Regex =
                Regex::new(r##"^([\S^=]+)="([^"]*)"(\s+(.*))?$"##).unwrap();
        }

        trace!("from_list: processing line: '{}'", line);
        let mut curr_pos = line;
        let mut params: HashMap<String, String> = HashMap::new();

        // parse current line into hashmap
        loop {
            trace!("parsing '{}'", curr_pos);
            if let Some(captures) = PARAM_RE.captures(curr_pos) {
                let param_name = captures.get(1).unwrap().as_str();
                let param_value = captures.get(2).unwrap().as_str();

                if !param_value.is_empty() {
                    params.insert(String::from(param_name), String::from(param_value));
                }

                if let Some(ref rest) = captures.get(4) {
                    curr_pos = rest.as_str();
                    trace!(
                        "Found param: '{}', value '{}', rest '{}'",
                        param_name,
                        param_value,
                        curr_pos
                    );
                } else {
                    trace!(
                        "Found param: '{}', value '{}', rest None",
                        param_name,
                        param_value
                    );
                    break;
                }
            } else {
                warn!("Failed to parse '{}'", curr_pos);
                return Err(MigError::displayed());
            }
        }

        let dev_type = LsblkInfo::get_str(&params, "TYPE")?;

        trace!("got type: '{}'", dev_type);

        match dev_type.as_str() {
            "disk" => Ok(Some(ResultType::Drive(LsblkDevice {
                name: LsblkInfo::get_str(&params, "NAME")?,
                kname: LsblkInfo::get_str(&params, "KNAME")?,
                maj_min: LsblkInfo::get_str(&params, "MAJ:MIN")?,
                uuid: if let Some(uuid) = params.get("UUID") {
                    Some(uuid.clone())
                } else {
                    None
                },
                size: LsblkInfo::get_u64(&params, "SIZE")?,
                children: None,
            }))),
            "part" => {
                Ok(Some(ResultType::Partition(LsblkPartition {
                    name: LsblkInfo::get_str(&params, "NAME")?,
                    kname: LsblkInfo::get_str(&params, "KNAME")?,
                    maj_min: LsblkInfo::get_str(&params, "MAJ:MIN")?,
                    fstype: if let Some(fstype) = params.get("FSTYPE") {
                        Some(fstype.clone())
                    } else {
                        None
                    },
                    mountpoint: LsblkInfo::get_pathbuf_or_none(&params, "MOUNTPOINT"),
                    label: if let Some(label) = params.get("LABEL") {
                        Some(label.clone())
                    } else {
                        None
                    },
                    uuid: if let Some(uuid) = params.get("UUID") {
                        Some(uuid.clone())
                    } else {
                        None
                    },
                    ro: LsblkInfo::get_str(&params, "RO")?,
                    size: LsblkInfo::get_u64(&params, "SIZE")?,
                    parttype: None,
                    partlabel: if let Some(label) = params.get("LABEL") {
                        Some(label.clone())
                    } else {
                        None
                    },
                    partuuid: if let Some(partuuid) = params.get("PARTUUID") {
                        trace!("Adding partuuid: {}", &partuuid);
                        Some(partuuid.clone())
                    } else {
                        trace!("Not adding partuuid");
                        None
                    },
                    // TODO: bit dodgy this one
                    index: None,
                })))
            }

            _ => {
                warn!("not processing line, type unknown: '{}'", line);
                Ok(None)
            }
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
