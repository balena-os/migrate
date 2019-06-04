use crate::{
    common::{call, MigError, MigErrorKind},
    mswin::wmi_utils::{WmiUtils, LogicalDrive}
};

const DRIVE_LETTERS: &[&str] = &[
    "D:", "E:", "F:", "G:", "H:", "I:", "J:", "K:", "L:", "M:", "N:", "O:", "P:", "Q:", "R:", "S:",
    "t:", "U:", "V:", "W:", "X:", "Y:", "Z:",
];

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


