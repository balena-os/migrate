use std::path::{PathBuf, Path};
use lazy_static::lazy_static;
use regex::{Regex};


use crate::{
    common::{call, MigError, MigErrorKind},
    mswin::wmi_utils::{LogicalDrive, WmiUtils},
};

const DRIVE_LETTERS: &[&str] = &[
    "D:", "E:", "F:", "G:", "H:", "I:", "J:", "K:", "L:", "M:", "N:", "O:", "P:", "Q:", "R:", "S:",
    "t:", "U:", "V:", "W:", "X:", "Y:", "Z:",
];


const MS2LINUX_PATH_RE: &str = r#"^\\\\\?\\[a-z,A-Z]:(.*)$"#;

pub(crate) fn to_linux_path(path: &Path) -> PathBuf {
    lazy_static! {
            static ref MS2LINUX_PATH_REGEX: Regex = Regex::new(MS2LINUX_PATH_RE).unwrap();
        }

    if let Some(captures) = MS2LINUX_PATH_REGEX.captures(&path.to_string_lossy()) {
        PathBuf::from(captures.get(1).unwrap().as_str())
    } else {
        PathBuf::from(path)
    }    
}

pub(crate) fn mount_efi() -> Result<LogicalDrive, MigError> {
    let drive_letters = WmiUtils::query_drive_letters()?;
    let mut mount_path: Option<&str> = None;
    for letter in DRIVE_LETTERS {
        if let None = drive_letters
            .iter()
            .position(|ltr| ltr.eq_ignore_ascii_case(letter))
        {
            mount_path = Some(*letter);
            break;
        }
    }

    if let Some(mount_path) = mount_path {
        let cmd_res = call("mountvol", &[mount_path, "/S"], true)?;
        if cmd_res.status.success() {
            Ok(LogicalDrive::query_for_name(mount_path)?)
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &format!("Failed to mount EFI drive on '{}'", mount_path),
            ));
        }
    } else {
        return Err(MigError::from_remark(
            MigErrorKind::InvState,
            "Unable to find a free drive letter to mount the EFI drive on",
        ));
    }
}
