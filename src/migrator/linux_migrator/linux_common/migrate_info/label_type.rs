use log::warn;
use regex::Regex;
use std::path::Path;

use crate::{
    common::{MigError, MigErrorKind},
    linux_migrator::linux_common::{ensured_commands::EnsuredCommands, FDISK_CMD, PARTED_CMD},
};

const DISK_LABEL_REGEX: &str = r#"^Disklabel type:\s*(\S+)$"#;

#[derive(Debug)]
pub(crate) enum LabelType {
    GPT,
    DOS,
    OTHER,
}

// TODO: Try for parted if fdisk does not supply label info

impl LabelType {
    pub fn from_device<P: AsRef<Path>>(
        cmds: &EnsuredCommands,
        device_path: P,
    ) -> Result<LabelType, MigError> {
        let device_path = device_path.as_ref();

        let disk_label_type = if cmds.has(PARTED_CMD) {
            // use parted
            let cmd_res = cmds.call(
                PARTED_CMD,
                &["-m", &device_path.to_string_lossy(), "print"],
                true,
            )?;

            if cmd_res.status.success() {
                let lines: Vec<&str> = cmd_res.stdout.lines().collect();
                if lines.len() < 2 {
                    return Err(MigError::from_remark(
                            MigErrorKind::InvParam,
                            &format!(
                                "new: failed to parse {} partition information from root device '{}', not enough lines in output",
                                device_path.display(),
                                PARTED_CMD
                            ),
                        ));
                }

                let words: Vec<&str> = lines[1].split(':').collect();
                if words.len() < 6 {
                    return Err(MigError::from_remark(
                            MigErrorKind::InvParam,
                            &format!(
                                "new: failed to parse {} partition information from root device '{}', not enough items in output",
                                device_path.display(),
                                PARTED_CMD
                            ),
                        ));
                }

                String::from(words[5])
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "new: failed to retrieve partition information from root device '{}'",
                        device_path.display()
                    ),
                ));
            }
        } else {
            // use fdisk
            if cmds.has(FDISK_CMD) {
                let cmd_res =
                    cmds.call(FDISK_CMD, &["-l", &device_path.to_string_lossy()], true)?;

                if cmd_res.status.success() {
                    let disk_lbl_re = Regex::new(DISK_LABEL_REGEX).unwrap();
                    let mut disk_label_type: Option<&str> = None;

                    for line in cmd_res.stdout.lines() {
                        if let Some(captures) = disk_lbl_re.captures(line) {
                            disk_label_type = Some(captures.get(1).unwrap().as_str());
                            break;
                        }
                    }

                    if let Some(disk_label_type) = disk_label_type {
                        String::from(disk_label_type)
                    } else {
                        warn!("No Disk Label information found in fdisk output, assuming 'dos'");
                        return Ok(LabelType::DOS);
                    }
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "new: failed to retrieve partition information from root device '{}'",
                            device_path.display()
                        ),
                    ));
                }
            } else {
                return Err(MigError::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "new: failed to retrieve partition information from root device '{}', no commands available",
                            device_path.display()
                        ),
                    ));
            }
        };

        match disk_label_type.as_ref() {
            "gpt" => Ok(LabelType::GPT),
            "dos" => Ok(LabelType::DOS),
            "msdos" => Ok(LabelType::DOS),
            _ => Ok(LabelType::OTHER),
        }
    }
}
