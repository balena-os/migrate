use failure::ResultExt;
use log::{debug, trace, warn};
use regex::Regex;
use serde::{
    Deserialize, Deserializer,
            de::{self, Unexpected},
};
use serde_json;

use std::fmt;
use std::path::{Path, PathBuf};

use crate::{
    common::{MigErrCtx, MigError, MigErrorKind},
    linux::{EnsuredCmds, LSBLK_CMD},
};

// const GPT_EFI_PART: &str = "C12A7328-F81F-11D2-BA4B-00A0C93EC93B";

const BLOC_DEV_SUPP_MAJ_NUMBERS: [&str; 45] = [
    "3", "8", "9", "21", "33", "34", "44", "48", "49", "50", "51", "52", "53", "54", "55", "56",
    "57", "58", "64", "65", "66", "67", "68", "69", "70", "71", "72", "73", "74", "75", "76", "77",
    "78", "79", "80", "81", "82", "83", "84", "85", "86", "87", "179", "180", "259",
];


struct DeserializeU64OrStringVisitor;
impl<'de> de::Visitor<'de> for DeserializeU64OrStringVisitor {
    type Value = u64;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("an integer or a string")
    }
    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
        where
            E: de::Error, {
        Ok(v)
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
    {
        match v.parse::<u64>() {
            Ok(val) => Ok(val),
            Err(_why) => {
                Err(E::invalid_value(Unexpected::Str(v), &self))
            }
        }
    }
}

fn deserialize_u64_or_string<'de, D>(deserializer: D) -> Result<u64, D::Error>
    where
        D: Deserializer<'de>,
{
    deserializer.deserialize_any(DeserializeU64OrStringVisitor)
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct LsblkPartition {
    pub name: String,
    pub kname: String,
    #[serde(rename(deserialize = "maj:min"))]
    pub maj_min: String,
    pub ro: String,
    pub uuid: Option<String>,
    pub fstype: Option<String>,
    pub mountpoint: Option<PathBuf>,
    pub label: Option<String>,
    pub parttype: Option<String>,
    pub partlabel: Option<String>,
    pub partuuid: Option<String>,
    #[serde(deserialize_with = "deserialize_u64_or_string")]
    pub size: u64,
    #[serde(skip)]
    pub index: Option<u16>,
}

impl LsblkPartition {
    pub fn get_path(&self) -> PathBuf {
        PathBuf::from(&format!("/dev/{}", self.name))
    }
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct LsblkDevice {
    pub name: String,
    pub kname: String,
    #[serde(rename(deserialize = "maj:min"))]
    pub maj_min: String,
    pub uuid: Option<String>,
    #[serde(deserialize_with = "deserialize_u64_or_string")]
    pub size: u64,
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

#[derive(Debug, Deserialize)]
pub(crate) struct LsblkInfo {
    blockdevices: Vec<LsblkDevice>,
}

impl<'a> LsblkInfo {
    pub fn for_device(device: &Path, cmds: &EnsuredCmds) -> Result<LsblkDevice, MigError> {
        let lsblk_info = LsblkInfo::call_lsblk(Some(device), cmds)?;
        if lsblk_info.blockdevices.len() == 1 {
            Ok(lsblk_info.blockdevices[0].clone())
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvState,
                &format!(
                    "Invalid number of devices found for device query: {}",
                    lsblk_info.blockdevices.len()
                ),
            ))
        }
    }

    pub fn all(cmds: &EnsuredCmds) -> Result<LsblkInfo, MigError> {
        let mut lsblk_info = LsblkInfo::call_lsblk(None, cmds)?;

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

    pub fn get_path_info<P: AsRef<Path>>(
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
            if let Some(ref children) = device.children {
                for part in children {
                    if let Some(ref mountpoint) = part.mountpoint {
                        if abs_path == Path::new(mountpoint) {
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

    pub fn get_blk_devices(&'a self) -> &'a Vec<LsblkDevice> {
        &self.blockdevices
    }

    // get the LsblkDevice & LsblkPartition from partition device path as in /dev/sda1
    pub fn get_devinfo_from_partition<P: AsRef<Path>>(
        &'a self,
        part_path: P,
    ) -> Result<(&'a LsblkDevice, &'a LsblkPartition), MigError> {
        let part_path = part_path.as_ref();
        trace!("get_devinfo_from_partition: '{}", part_path.display());
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

    fn call_lsblk(device: Option<&Path>, cmds: &EnsuredCmds) -> Result<LsblkInfo, MigError> {
        let mut _tmp_path: Option<String> = None;
        let args: Vec<&str> = if let Some(device) = device {
            _tmp_path = Some(String::from(&*device.to_string_lossy()));
            vec!["-b", "-O", "--json", _tmp_path.as_ref().unwrap()]
        } else {
            vec!["-b", "-O", "--json"]
        };

        let cmd_res = cmds.call(LSBLK_CMD, &args, true)?;
        if cmd_res.status.success() {
            Ok(LsblkInfo::from_json(&cmd_res.stdout)?)
        } else {
            let args: Vec<&str> = if let Some(device) = device {
                _tmp_path = Some(String::from(&*device.to_string_lossy()));
                vec![
                    "-b",
                    "-P",
                    "-o",
                    "NAME,KNAME,MAJ:MIN,FSTYPE,MOUNTPOINT,LABEL,UUID,RO,SIZE,TYPE",
                    _tmp_path.as_ref().unwrap(),
                ]
            } else {
                vec![
                    "-b",
                    "-P",
                    "-o",
                    "NAME,KNAME,MAJ:MIN,FSTYPE,MOUNTPOINT,LABEL,UUID,RO,SIZE,TYPE",
                ]
            };

            let cmd_res = cmds.call(LSBLK_CMD, &args, true)?;
            if cmd_res.status.success() {
                Ok(LsblkInfo::from_list(&cmd_res.stdout)?)
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::ExecProcess,
                    "new: failed to determine block device attributes for",
                ));
            }
        }
    }

    fn from_list(list: &str) -> Result<LsblkInfo, MigError> {
        let param_re = Regex::new(r#"^([^=]+)="([^"]*)"$"#).unwrap();

        let parse_it = |word: &str, expect: &str| -> Result<String, MigError> {
            trace!("parse_it: word: '{}', expect: '{}'", word, expect);
            if let Some(captures) = param_re.captures(word) {
                let name = captures.get(1).unwrap().as_str();
                if name == expect {
                    Ok(String::from(captures.get(2).unwrap().as_str()))
                } else {
                    Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!(
                            "Unexpected parameter encountered: expected '{}' got  '{}'",
                            expect, name
                        ),
                    ))
                }
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!("Failed to parse lsblk output param: '{}'", word),
                ))
            }
        };

        let parse_u64 = |s: String| -> Result<u64,MigError> {
            Ok(s.parse::<u64>().context(MigErrCtx::from_remark(MigErrorKind::Upstream,&format!("Failed to parse u64 from string '{}'", s)))?)
        };

        let string_or_none = |s: String| -> Option<String> {
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        };

        let pathbuf_or_none = |s: String| -> Option<PathBuf> {
            if s.is_empty() {
                None
            } else {
                Some(PathBuf::from(s))
            }
        };

        let mut lsblk_info: LsblkInfo = LsblkInfo {
            blockdevices: Vec::new(),
        };

        let mut curr_dev: Option<LsblkDevice> = None;

        for line in list.lines() {
            debug!("from_list: processing line: '{}'", line);

            let words: Vec<&str> = line.split_whitespace().collect();

            if words.len() != 10 {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "Failed to parse lsblk output: '{}' invalid word count: {}",
                        line,
                        words.len()
                    ),
                ));
            }

            let dev_type = parse_it(words[9], "TYPE")?;
            debug!("got type: '{}'", dev_type);

            match dev_type.as_str() {
                "disk" => {
                    if let Some(curr_dev) = curr_dev {
                        lsblk_info.blockdevices.push(curr_dev);
                    }

                    curr_dev = Some(LsblkDevice {
                        name: parse_it(words[0], "NAME")?,
                        kname: parse_it(words[1], "KNAME")?,
                        maj_min: parse_it(words[2], "MAJ:MIN")?,
                        uuid: string_or_none(parse_it(words[6], "UUID")?),
                        size: parse_u64(parse_it(words[8], "SIZE")?)?,
                        children: None,
                    });
                }
                "part" => {
                    if let Some(ref mut curr_dev) = curr_dev {
                        let children = if let Some(ref mut children) = curr_dev.children {
                            children
                        } else {
                            curr_dev.children = Some(Vec::new());
                            curr_dev.children.as_mut().unwrap()
                        };

                        children.push(LsblkPartition {
                            name: parse_it(words[0], "NAME")?,
                            kname: parse_it(words[1], "KNAME")?,
                            maj_min: parse_it(words[2], "MAJ:MIN")?,
                            fstype: string_or_none(parse_it(words[3], "FSTYPE")?),
                            mountpoint: pathbuf_or_none(parse_it(words[4], "MOUNTPOINT")?),
                            label: string_or_none(parse_it(words[5], "LABEL")?),
                            uuid: string_or_none(parse_it(words[6], "UUID")?),
                            ro: parse_it(words[7], "RO")?,
                            size: parse_u64(parse_it(words[8], "SIZE")?)?,
                            parttype: None,
                            partlabel: None,
                            partuuid: None,
                            // TODO: bit dodgy this one
                            index: Some((children.len() + 1) as u16),
                        });
                    } else {
                        return Err(MigError::from_remark(
                            MigErrorKind::InvState,
                            &format!(
                                "Invalid state while parsing lsblk output line '{}', no device",
                                line
                            ),
                        ));
                    };
                }

                _ => debug!("not processing line, type unknown: '{}'", line),
            }

            debug!("lsblk line as words: '{:?}'", words);
        }

        if let Some(curr_dev) = curr_dev {
            lsblk_info.blockdevices.push(curr_dev);
            // curr_dev = None;
        }

        Ok(lsblk_info)
    }

    fn from_json(json_str: &str) -> Result<LsblkInfo, MigError> {
        let mut lsblk_info =
            serde_json::from_str::<LsblkInfo>(json_str).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                "failed to deserialze lsblk output from json",
            ))?;

        for device in &mut lsblk_info.blockdevices {
            if let Some(ref mut children) = device.children {
                let mut count: u16 = 1;
                for mut partition in children {
                    partition.index = Some(count);
                    count += 1;
                }
            }
        }

        Ok(lsblk_info)
    }
}
