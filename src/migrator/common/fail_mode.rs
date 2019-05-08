use serde::{Deserialize};
use crate::common::{MigError, MigErrorKind};

#[derive(Debug, Clone, Deserialize)]
pub(crate) enum FailMode {
    Reboot,
    RescueShell,
}

impl FailMode {
    pub(crate) fn from_str(val: &str) -> Result<&'static FailMode, MigError> {
        let lc_val = val.to_lowercase();
        if lc_val == "rescueshell" {
            Ok(&FailMode::RescueShell)
        } else if lc_val == "reboot" {
            Ok(&FailMode::Reboot)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("Invalid value for FailMode {}", val),
            ))
        }
    }

    pub(crate) fn to_string(&self) -> &'static str {
        match self {
            FailMode::Reboot => "Reboot",
            FailMode::RescueShell => "RescueShell",
        }
    }

    pub(crate) fn get_default() -> &'static FailMode {
        &FailMode::RescueShell
    }
}
