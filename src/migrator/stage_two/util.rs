use lazy_static::lazy_static;
use std::collections::HashMap;
use log::{error};

use crate::common::{MigError, CmdRes};
use crate::linux_common::{call_cmd_from, whereis};

pub const DF_CMD: &str = "df";
pub const LSBLK_CMD: &str = "lsblk";
pub const MOUNT_CMD: &str = "mount";
pub const FILE_CMD: &str = "file";
pub const UNAME_CMD: &str = "uname";
pub const MOKUTIL_CMD: &str = "mokutil";
pub const GRUB_INSTALL_CMD: &str = "grub-install";
pub const REBOOT_CMD: &str = "reboot";
pub const CHMOD_CMD: &str = "chmod";

const REQUIRED_CMDS: &'static [&'static str] = &[DF_CMD, LSBLK_CMD, MOUNT_CMD, FILE_CMD, UNAME_CMD, REBOOT_CMD, CHMOD_CMD];
const OPTIONAL_CMDS: &'static [&'static str] = &[MOKUTIL_CMD, GRUB_INSTALL_CMD];


pub(crate) fn call_cmd(cmd: &str, args: &[&str], trim_stdout: bool) -> Result<CmdRes, MigError> {
    lazy_static! {
        static ref CMD_PATH: HashMap<String,Option<String>> = {
            let mut map = HashMap::new();
            for cmd in REQUIRED_CMDS {
                map.insert(
                    String::from(*cmd),
                    Some(match whereis(cmd) {
                        Ok(cmd) => cmd,
                        Err(_why) => {
                            let message = format!("cannot find required command {}", cmd);
                            error!("{}", message);
                            panic!("{}", message);
                        }
                    }));
            }
            for cmd in OPTIONAL_CMDS {
                map.insert(
                    String::from(*cmd),
                    match whereis(cmd) {
                        Ok(cmd) => Some(cmd),
                        Err(_why) => None, // TODO: check error codes
                    });
            }
            map
        };
    }

    call_cmd_from(&CMD_PATH, cmd, args, trim_stdout)
}