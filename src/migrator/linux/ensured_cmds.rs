use crate::{
    common::{call, CmdRes, MigError, MigErrorKind},
    linux::linux_common::whereis,
};
use log::{trace, warn};
use std::collections::HashMap;

pub const DF_CMD: &str = "df";
pub const LSBLK_CMD: &str = "lsblk";
pub const FDISK_CMD: &str = "fdisk";
pub const FILE_CMD: &str = "file";
pub const UNAME_CMD: &str = "uname";
pub const MOUNT_CMD: &str = "mount";
pub const MOKUTIL_CMD: &str = "mokutil";
pub const GRUB_UPDT_CMD: &str = "update-grub";
pub const GRUB_REBOOT_CMD: &str = "grub-reboot";
pub const REBOOT_CMD: &str = "reboot";
pub const CHMOD_CMD: &str = "chmod";
pub const DD_CMD: &str = "dd";
pub const PARTPROBE_CMD: &str = "partprobe";
pub const PARTED_CMD: &str = "parted";
pub const GZIP_CMD: &str = "gzip";
pub const MKTEMP_CMD: &str = "mktemp";

#[derive(Debug)]
pub(crate) struct EnsuredCmds {
    cmd_table: HashMap<String, String>,
}

impl EnsuredCmds {
    pub fn new(cmds: &[&str]) -> Result<EnsuredCmds, MigError> {
        let mut ensured_cmds = EnsuredCmds {
            cmd_table: HashMap::new(),
        };
        ensured_cmds.ensure_cmds(cmds)?;
        Ok(ensured_cmds)
    }

    pub fn ensure_cmds(&mut self, cmds: &[&str]) -> Result<(), MigError> {
        for cmd in cmds {
            if let Ok(cmd_path) = whereis(cmd) {
                self.cmd_table.insert(String::from(*cmd), cmd_path.clone());
            } else {
                let message = format!("cannot find required command {}", cmd);
                warn!("{}", message);
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!("{}", message),
                ));
            }
        }
        Ok(())
    }

    pub fn ensure(&mut self, cmd: &str) -> Result<String, MigError> {
        if let Ok(cmd_path) = whereis(cmd) {
            self.cmd_table.insert(String::from(cmd), cmd_path.clone());
            Ok(cmd_path)
        } else {
            let message = format!("cannot find command {}", cmd);
            warn!("{}", message);
            Err(MigError::from_remark(MigErrorKind::NotFound, &message))
        }
    }

    pub fn has<'a>(&'a self, cmd: &str) -> bool {
        if let Some(_cmd_path) = self.cmd_table.get(cmd) {
            true
        } else {
            false
        }
    }

    pub fn get<'a>(&'a self, cmd: &str) -> Result<&'a str, MigError> {
        if let Some(cmd_path) = self.cmd_table.get(cmd) {
            Ok(cmd_path)
        } else {
            Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("The command is not a checked command: {}", cmd),
            ))
        }
    }

    pub fn call(&self, cmd: &str, args: &[&str], trim_stdout: bool) -> Result<CmdRes, MigError> {
        trace!(
            "call_cmd: entered with cmd: '{}', args: {:?}, trim: {}",
            cmd,
            args,
            trim_stdout
        );

        Ok(call(self.get(cmd)?, args, trim_stdout)?)
    }
}
