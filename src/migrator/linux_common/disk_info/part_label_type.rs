use regex::{Captures, Regex};
use std::path::Path;

use crate::{
    common::{MigError, MigErrorKind},
    linux_common::{call_cmd, FDISK_CMD},
};

const DISK_LABEL_REGEX: &str = r#"^Disklabel type:\s*(\S+)$"#;

pub(crate) enum PartLabelType {
    GPT,
    DOS,
    OTHER,
}

impl PartLabelType {
    pub fn from_device(device_path: &Path) -> Result<PartLabelType, MigError> {
        let cmd_res = call_cmd(FDISK_CMD, &["-l", &device_path.to_string_lossy()], true)?;

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
                match disk_label_type.as_str() {
                    "gpt" => Ok(PartLabelType::GPT),
                    "dos" => Ok(PartLabelType::DOS),
                    _ => Ok(PartLabelType::OTHER),
                }
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "new: failed to retrieve partition type information from device '{}'",
                        device_path.display()
                    ),
                ))
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "new: failed to retrieve partition information from root device '{}'",
                    device_path.display()
                ),
            ))
        }
    }
}
