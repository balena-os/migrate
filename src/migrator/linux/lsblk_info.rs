use failure::ResultExt;
use lazy_static::lazy_static;
use log::{debug, trace, warn};
use regex::Regex;
use std::path::{Path, PathBuf};

//use crate::linux::linux_common::to_std_device_path;
use crate::linux::linux_defs::UDEVADM_CMD;
use crate::{
    common::{call, MigErrCtx, MigError, MigErrorKind},
    // defs::{DISK_BY_LABEL_PATH, DISK_BY_PARTUUID_PATH, DISK_BY_UUID_PATH},
    linux::linux_defs::LSBLK_CMD,
};
use std::collections::HashMap;

pub(crate) mod partition;
use partition::Partition;

pub(crate) mod block_device;
use block_device::BlockDevice;

// const GPT_EFI_PART: &str = "C12A7328-F81F-11D2-BA4B-00A0C93EC93B";

// TODO: take care of updating mountpoints, eg wih findmnt

const BLOC_DEV_SUPP_MAJ_NUMBERS: [&str; 45] = [
    "3", "8", "9", "21", "33", "34", "44", "48", "49", "50", "51", "52", "53", "54", "55", "56",
    "57", "58", "64", "65", "66", "67", "68", "69", "70", "71", "72", "73", "74", "75", "76", "77",
    "78", "79", "80", "81", "82", "83", "84", "85", "86", "87", "179", "180", "259",
];

const LSBLK_COLS: &str = "NAME,KNAME,MAJ:MIN,FSTYPE,MOUNTPOINT,LABEL,UUID,SIZE,RO,TYPE";

#[allow(dead_code)]
enum ResultType {
    Drive(BlockDevice),
    Partition(Partition),
}

pub struct ResultParams {
    param_map: HashMap<String, String>,
}

impl<'a> ResultParams {
    pub fn new(param_map: HashMap<String, String>) -> ResultParams {
        ResultParams { param_map }
    }

    pub fn get_str(&'a self, name: &str) -> Result<&'a str, MigError> {
        if let Some(result) = self.param_map.get(name) {
            Ok(result.as_str())
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("get_str: name was not found in result params: {}", name),
            ))
        }
    }

    pub fn get_opt_str(&'a self, name: &str) -> Option<String> {
        if let Some(result) = self.param_map.get(name) {
            Some(result.clone())
        } else {
            None
        }
    }

    pub fn get_u64(&'a self, name: &str) -> Result<u64, MigError> {
        if let Some(res) = self.param_map.get(name) {
            Ok(res.parse::<u64>().context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to parse u64 from '{}'", res),
            ))?)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("get_str: name was not found in result params: {}", name),
            ))
        }
    }

    #[allow(dead_code)]
    pub fn get_opt_u64(&'a self, name: &str) -> Result<Option<u64>, MigError> {
        if let Some(res) = self.param_map.get(name) {
            Ok(Some(res.parse::<u64>().context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to parse u64 from '{}'", name),
            ))?))
        } else {
            Ok(None)
        }
    }

    pub fn get_u16(&'a self, name: &str) -> Result<u16, MigError> {
        if let Some(res) = self.param_map.get(name) {
            Ok(res.parse::<u16>().context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to parse u64 from '{}'", res),
            ))?)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("get_str: name was not found in result params: {}", name),
            ))
        }
    }

    pub fn get_opt_pathbuf(&self, name: &str) -> Option<PathBuf> {
        if let Some(res) = self.param_map.get(name) {
            Some(PathBuf::from(res))
        } else {
            None
        }
    }
    /*    pub fn get_alt_path(&self) -> PathBuf {
            if let Some(ref partuuid) = self.partuuid {
                path_append(DISK_BY_PARTUUID_PATH, partuuid)
            } else if let Some(ref uuid) = self.uuid {
                path_append(DISK_BY_UUID_PATH, uuid)
            } else if let Some(ref label) = self.label {
                path_append(DISK_BY_LABEL_PATH, label)
            } else {
                path_append("/dev", &self.name)
            }
        }
    */
}

fn call_lsblk_all() -> Result<Vec<ResultParams>, MigError> {
    call_lsblk(vec!["-b", "-P", "-o", LSBLK_COLS])
}

fn call_lsblk_for<P: AsRef<Path>>(device: P) -> Result<Vec<ResultParams>, MigError> {
    call_lsblk(vec![
        "-b",
        "-P",
        "-o",
        LSBLK_COLS,
        &*device.as_ref().to_string_lossy(),
    ])
}

fn call_lsblk(args: Vec<&str>) -> Result<Vec<ResultParams>, MigError> {
    debug!("call_lsblk: with args {:?}", args);
    let lsblk_cmd_res = call(LSBLK_CMD, &args, true)?;
    trace!(
        "call_lsblk: call success {}",
        lsblk_cmd_res.status.success()
    );
    if lsblk_cmd_res.status.success() {
        let mut lsblk_results: Vec<ResultParams> = Vec::new();
        for line in lsblk_cmd_res.stdout.lines() {
            //.skip(1)
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

fn call_udevadm<P: AsRef<Path>>(device: P) -> Result<ResultParams, MigError> {
    debug!("call_udevadm: with device {:?}", device.as_ref().display());
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

    trace!(
        "call_udevadm: call success {}",
        udev_cmd_res.status.success()
    );

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

        Ok(ResultParams::new(udev_result))
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

fn parse_lsblk_line(line: &str) -> Result<ResultParams, MigError> {
    trace!("parse_line: called with '{}'", line);

    lazy_static! {
        static ref LSBLK_PARAM_RE: Regex =
            Regex::new(r##"^([\S^=]+)="([^"]*)"(\s+(.*))?$"##).unwrap();
    }

    trace!("from_list: processing line: '{}'", line);
    let mut curr_pos = line;
    let mut result: HashMap<String, String> = HashMap::new();

    // parse current line into hashmap
    loop {
        trace!("parsing '{}'", curr_pos);
        if let Some(captures) = LSBLK_PARAM_RE.captures(curr_pos) {
            let param_name = captures.get(1).unwrap().as_str();
            let param_value = captures.get(2).unwrap().as_str();

            if !param_value.is_empty() {
                result.insert(String::from(param_name), String::from(param_value));
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
            warn!("Failed to parse lsblk output '{}'", curr_pos);
            return Err(MigError::displayed());
        }
    }

    Ok(ResultParams::new(result))
}

#[derive(Debug, Clone)]
pub(crate) struct LsblkInfo {
    blockdevices: Vec<BlockDevice>,
}

impl<'a> LsblkInfo {
    pub fn new() -> Result<LsblkInfo, MigError> {
        debug!("new:");
        let lsblk_results = call_lsblk_all()?;
        let mut lsblk_info = LsblkInfo {
            blockdevices: Vec::new(),
        };

        let maj_min_re = Regex::new(r#"^(\d+):\d+$"#).unwrap();
        for lsblk_result in lsblk_results {
            //let udev_result = call_udevadm(lsblk_result.get_str("NAME")?)?;
            match lsblk_result.get_str("TYPE")? {
                "part" => {
                    if let Some(block_device) = lsblk_info.blockdevices.last_mut() {
                        if let Some(ref mut children) = block_device.children {
                            children.push(Partition::new(&lsblk_result)?);
                        } else {
                            block_device.children = Some(vec![Partition::new(&lsblk_result)?]);
                        }
                    } else {
                        return Err(MigError::from_remark(
                            MigErrorKind::InvParam,
                            "No existing block device found for partition type lsblk result",
                        ));
                    }
                }
                "disk" => {
                    let dev_maj_min = lsblk_result.get_str("MAJ:MIN")?;
                    if let Some(captures) = maj_min_re.captures(dev_maj_min) {
                        let this_maj = captures.get(1).unwrap().as_str();

                        #[allow(clippy::search_is_some)]
                        let res = BLOC_DEV_SUPP_MAJ_NUMBERS
                            .iter()
                            .find(|sup_maj| this_maj == **sup_maj)
                            .is_some();
                        if res {
                            lsblk_info
                                .blockdevices
                                .push(BlockDevice::new(&lsblk_result)?);
                        } else {
                            warn!("Unsupported device maj:min: {}", dev_maj_min);
                            continue;
                        }
                    } else {
                        warn!("Unsupported device maj:min: {}", dev_maj_min);
                        continue;
                    }
                }
                _ => {
                    //warn!("Invalid device type: {}", lsblk_result.get_str("TYPE")?);
                    continue;
                }
            }
        }

        Ok(lsblk_info)
    }

    /*pub fn get_blk_devices(&'a self) -> &'a Vec<BlockDevice> {
        &self.blockdevices
    }*/

    pub fn get_devices_for_partuuid(
        &'a self,
        partuuid: &str,
    ) -> Result<(&'a BlockDevice, &'a Partition), MigError> {
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
    ) -> Result<(&'a BlockDevice, &'a Partition), MigError> {
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
    ) -> Result<(&'a BlockDevice, &'a Partition), MigError> {
        debug!("get_devices_for_label: {}", label);
        for device in &self.blockdevices {
            if let Some(ref children) = device.children {
                if let Some(partition) = children.iter().find(|part| {
                    if let Some(ref curr_label) = part.label {
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
    ) -> Result<(&'a BlockDevice, &'a Partition), MigError> {
        let path = path.as_ref();
        debug!("get_devices_for_path: '{}", path.display());
        let abs_path = path.canonicalize().context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("failed to canonicalize path: '{}'", path.display()),
        ))?;

        let mut mp_match: Option<(&BlockDevice, &Partition)> = None;

        for device in &self.blockdevices {
            trace!(
                "get_devices_for_path: looking at device '{}",
                device.get_path().display()
            );
            if let Some(ref children) = device.children {
                for part in children {
                    trace!(
                        "get_devices_for_path: looking at partition '{}",
                        part.get_path().display()
                    );
                    if let Some(ref mountpoint) = part.mountpoint {
                        trace!(
                            "Comparing search path '{}' to mountpoint '{}'",
                            abs_path.display(),
                            mountpoint.display()
                        );
                        if abs_path == PathBuf::from(mountpoint) {
                            trace!(
                                "get_devices_for_path: partition mountpoint is search path '{}'",
                                mountpoint.display()
                            );
                            return Ok((&device, part));
                        } else if abs_path.starts_with(mountpoint) {
                            trace!(
                                "get_devices_for_path: partition mountpoint starts with search path '{}'",
                                mountpoint.display()
                            );

                            if let Some((_last_dev, last_part)) = mp_match {
                                if let Some(ref last_mp) = last_part.mountpoint {
                                    if last_mp.to_string_lossy().len()
                                        < mountpoint.to_string_lossy().len()
                                    {
                                        trace!(
                                            "get_devices_for_path: new best match for '{}' -> '{}'",
                                            abs_path.display(),
                                            mountpoint.display()
                                        );
                                        mp_match = Some((&device, part))
                                    }
                                } else {
                                    trace!(
                                        "get_devices_for_path: new best match for '{}' -> '{}'",
                                        abs_path.display(),
                                        mountpoint.display()
                                    );
                                    mp_match = Some((&device, part))
                                }
                            } else {
                                trace!(
                                    "get_devices_for_path: first match for '{}' -> '{}'",
                                    abs_path.display(),
                                    mountpoint.display()
                                );
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
    ) -> Result<(&'a BlockDevice, &'a Partition), MigError> {
        let part_path = part_path.as_ref();
        debug!("get_devices_for_partition: '{}", part_path.display());

        // let part_path = to_std_device_path(part_path)?;
        if let Some(part_name) = part_path.file_name() {
            let cmp_name = part_name.to_string_lossy();
            let mut result: Option<(&BlockDevice, &Partition)> = None;
            let _res = self.blockdevices.iter().find(|&dev| {
                if let Ok(partition) = dev.get_devinfo_from_part_name(&*cmp_name) {
                    result = Some((dev, partition));
                    true
                } else {
                    false
                }
            });
            if let Some(result) = result {
                Ok(result)
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

#[cfg(test)]
mod tests {
    use crate::linux::lsblk_info::parse_lsblk_line;

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
    fn read_output_ok1() {
        parse_lsblk_line(LSBLK_OUTPUT1).unwrap();
    }
    /*
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


        #[test]
        fn get_partition_by_label() -> () {
            let lsblk_info = LsblkInfo::from_string(LSBLK_OUTPUT1).unwrap();
            let (drive, partition) = lsblk_info.get_devices_for_label("ESP").unwrap();
            assert!(drive.name == "nvme0n1");
            assert!(partition.name == "nvme0n1p1");
        use crate::linux::lsblk_info::LsblkInfo;

        const LSBLK_OUTPUT1: &str = r##"NAME="loop0" KNAME="loop0" MAJ:MIN="7:0" FSTYPE="squashfs" MOUNTPOINT="/snap/core/7270" LABEL="" UUID="" RO="1" SIZE="92778496" TYPE="loop"
    NAME="loop1" KNAME="loop1" MAJ:MIN="7:1" FSTYPE="squashfs" MOUNTPOINT="/snap/core18/1066" LABEL="" UUID="" RO="1" SIZE="57069568" TYPE="loop"
    NAME="loop2" KNAME="loop2" MAJ:MIN="7:2" FSTYPE="squashfs" MOUNTPOINT="/snap/core18/1074" LABEL="" UUID="" RO="1" SIZE="57069568" TYPE="loop"
    NAME="loop3" KNAME="loop3" MAJ:MIN="7:3" FSTYPE="squashfs" MOUNTPOINT="/snap/gnome-3-28-1804/71" LABEL="" UUID="" RO="1" SIZE="157192192" TYPE="loop"
    NAME="loop4" KNAME="loop4" MAJ:MIN="7:4" FSTYPE="squashfs" MOUNTPOINT="/snap/core/7396" LABEL="" UUID="" RO="1" SIZE="92983296" TYPE="loop"
    NAME="loop5" KNAME="loop5" MAJ:MIN="7:5" FSTYPE="squashfs" MOUNTPOINT="/snap/gnome-logs/61" LABEL="" UUID="" RO="1" SIZE="1032192" TYPE="loop"
    NAME="loop6" KNAME="loop6" MAJ:MIN="7:6" FSTYPE="squashfs" MOUNTPOINT="/snap/gtk-common-themes/1313" LABEL="" UUID="" RO="1" SIZE="44879872" TYPE="loop"
    NAME="loop7" KNAME="loop7" MAJ:MIN="7:7" FSTYPE="squashfs" MOUNTPOINT="/snap/vlc/1049" LABEL="" UUID="" RO="1" SIZE="212713472" TYPE="loop"
    NAME="loop8" KNAME="loop8" MAJ:MIN="7:8" FSTYPE="squashfs" MOUNTPOINT="/snap/gnome-3-28-1804/67" LABEL="" UUID="" RO="1" SIZE="157184000" TYPE="loop"
    NAME="loop9" KNAME="loop9" MAJ:MIN="7:9" FSTYPE="squashfs" MOUNTPOINT="/snap/gnome-system-monitor/100" LABEL="" UUID="" RO="1" SIZE="3825664" TYPE="loop"
    NAME="loop10" KNAME="loop10" MAJ:MIN="7:10" FSTYPE="squashfs" MOUNTPOINT="/snap/gtk2-common-themes/5" LABEL="" UUID="" RO="1" SIZE="135168" TYPE="loop"
    NAME="loop11" KNAME="loop11" MAJ:MIN="7:11" FSTYPE="squashfs" MOUNTPOINT="/snap/gimp/189" LABEL="" UUID="" RO="1" SIZE="229728256" TYPE="loop"
    NAME="loop12" KNAME="loop12" MAJ:MIN="7:12" FSTYPE="squashfs" MOUNTPOINT="/snap/spotify/36" LABEL="" UUID="" RO="1" SIZE="189870080" TYPE="loop"
    NAME="loop13" KNAME="loop13" MAJ:MIN="7:13" FSTYPE="squashfs" MOUNTPOINT="/snap/gnome-characters/296" LABEL="" UUID="" RO="1" SIZE="15462400" TYPE="loop"
    NAME="loop14" KNAME="loop14" MAJ:MIN="7:14" FSTYPE="squashfs" MOUNTPOINT="/snap/gnome-calculator/406" LABEL="" UUID="" RO="1" SIZE="4218880" TYPE="loop"
    NAME="nvme0n1" KNAME="nvme0n1" MAJ:MIN="259:0" FSTYPE="" MOUNTPOINT="" LABEL="" UUID="" RO="0" SIZE="512110190592" TYPE="disk"
    NAME="nvme0n1p1" KNAME="nvme0n1p1" MAJ:MIN="259:1" FSTYPE="vfat" MOUNTPOINT="/boot/efi" LABEL="ESP SPACE" UUID="42D3-AAB8" RO="0" SIZE="713031680" TYPE="part"
    NAME="nvme0n1p2" KNAME="nvme0n1p2" MAJ:MIN="259:2" FSTYPE="" MOUNTPOINT="" LABEL="" UUID="" RO="0" SIZE="134217728" TYPE="part"
    NAME="nvme0n1p3" KNAME="nvme0n1p3" MAJ:MIN="259:3" FSTYPE="" MOUNTPOINT="" LABEL="" UUID="" RO="0" SIZE="79322677248" TYPE="part"
    NAME="nvme0n1p4" KNAME="nvme0n1p4" MAJ:MIN="259:4" FSTYPE="ntfs" MOUNTPOINT="" LABEL="WINRETOOLS" UUID="500EC0840EC06516" RO="0" SIZE="1038090240" TYPE="part"
    NAME="nvme0n1p5" KNAME="nvme0n1p5" MAJ:MIN="259:5" FSTYPE="ntfs" MOUNTPOINT="" LABEL="Image mit Space" UUID="C614C0AC14C0A0B3" RO="0" SIZE="10257170432" TYPE="part"
    NAME="nvme0n1p6" KNAME="nvme0n1p6" MAJ:MIN="259:6" FSTYPE="ntfs" MOUNTPOINT="" LABEL="DELLSUPPORT" UUID="AA88E9D888E9A2D5" RO="0" SIZE="1212153856" TYPE="part"
    NAME="nvme0n1p7" KNAME="nvme0n1p7" MAJ:MIN="259:7" FSTYPE="ext4" MOUNTPOINT="/" LABEL="" UUID="b305522d-faa7-49fc-a7d1-70dae48bcc3e" RO="0" SIZE="419430400000" TYPE="part"
    "##;

        #[test]
        fn read_output_ok1() {
            let _lsblk_info = LsblkInfo::from_list(LSBLK_OUTPUT1).unwrap();
        }
        */
}
