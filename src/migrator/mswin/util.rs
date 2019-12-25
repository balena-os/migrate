use lazy_static::lazy_static;
use log::debug;
use regex::Regex;
use std::path::{Path, PathBuf};

use crate::common::{file_exists, path_append};
use crate::{
    common::{call, MigError, MigErrorKind},
    mswin::wmi_utils::{LogicalDrive, WmiUtils},
};

const DRIVE_LETTERS: &[&str] = &[
    "D:", "E:", "F:", "G:", "H:", "I:", "J:", "K:", "L:", "M:", "N:", "O:", "P:", "Q:", "R:", "S:",
    "t:", "U:", "V:", "W:", "X:", "Y:", "Z:",
];

const CHECK_EFI_PATH: &str = r#"\EFI\Microsoft\boot\bootmgfw.efi"#;

const MS2LINUX_PATH_RE: &str = r#"^\\\\\?\\[a-z,A-Z]:(.*)$"#;

pub(crate) fn to_linux_path(path: &Path) -> PathBuf {
    lazy_static! {
        static ref MS2LINUX_PATH_REGEX: Regex = Regex::new(MS2LINUX_PATH_RE).unwrap();
    }

    let path_str = String::from(&*path.to_string_lossy());
    let path = if let Some(captures) = MS2LINUX_PATH_REGEX.captures(&path_str) {
        captures.get(1).unwrap().as_str()
    } else {
        &path_str
    };

    PathBuf::from(path.replace(r#"\"#, "/"))
}

// find or mount EFI LogicalDrive
pub(crate) fn mount_efi() -> Result<LogicalDrive, MigError> {
    // get drive letters in use
    let drive_letters = WmiUtils::query_drive_letters()?;

    if let Some(drive_letter) = drive_letters.iter().find(|dl| {
        debug!(
            "Checking path for EFI: '{}'",
            path_append(dl, CHECK_EFI_PATH).display()
        );
        file_exists(path_append(dl, CHECK_EFI_PATH))
    }) {
        // found EFI drive - return
        Ok(LogicalDrive::query_for_name(drive_letter)?)
    } else {
        // find free drive letter
        if let Some(drive_letter) = DRIVE_LETTERS.iter().find(|dl| {
            if let None = drive_letters.iter().find(|used| &used.as_str() == *dl) {
                true
            } else {
                false
            }
        }) {
            // mount EFI drive
            let cmd_res = call("mountvol", &[drive_letter, "/S"], true)?;
            if cmd_res.status.success() && cmd_res.stderr.is_empty() {
                Ok(LogicalDrive::query_for_name(drive_letter)?)
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::ExecProcess,
                    &format!(
                        "Failed to mount EFI drive on '{}', msg: '{}'",
                        drive_letter, cmd_res.stderr
                    ),
                ))
            }
        } else {
            Err(MigError::from_remark(
                MigErrorKind::NotFound,
                "Could not find a free drive letter for EFI device",
            ))
        }
    }
}
