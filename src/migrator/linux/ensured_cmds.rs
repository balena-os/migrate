use crate::{
    common::{call, CmdRes, MigError, MigErrorKind},
    linux::linux_common::whereis,
};
use log::{trace, warn};
use std::collections::HashMap;

pub const CHMOD_CMD: &str = "chmod";
pub const DD_CMD: &str = "dd";
pub const DF_CMD: &str = "df";
// pub const FDISK_CMD: &str = "fdisk";
pub const FILE_CMD: &str = "file";
pub const LSBLK_CMD: &str = "lsblk";
// pub const BLKID_CMD: &str = "blkid";
pub const GRUB_REBOOT_CMD: &str = "grub-reboot";
pub const GRUB_UPDT_CMD: &str = "update-grub";
pub const GZIP_CMD: &str = "gzip";
pub const MKTEMP_CMD: &str = "mktemp";
pub const MOKUTIL_CMD: &str = "mokutil";
pub const MOUNT_CMD: &str = "mount";
pub const LOSETUP_CMD: &str = "losetup";
pub const PARTED_CMD: &str = "parted";
pub const PARTPROBE_CMD: &str = "partprobe";
pub const REBOOT_CMD: &str = "reboot";
pub const TAR_CMD: &str = "tar";
pub const UDEVADM_CMD: &str = "udevadm";
pub const UNAME_CMD: &str = "uname";
pub const EXT_FMT_CMD: &str = "mkfs.ext4";
pub const FAT_FMT_CMD: &str = "mkfs.vfat";

pub const FAT_CHK_CMD: &str = "fsck.vfat";

#[derive(Debug)]
pub(crate) struct EnsuredCmds {
    cmd_table: HashMap<String, String>,
}

impl EnsuredCmds {
    pub fn new() -> EnsuredCmds {
        EnsuredCmds {
            cmd_table: HashMap::new(),
        }
    }

    pub fn ensure_cmds(&mut self, cmds: &[&str]) -> Result<(), MigError> {
        let mut result: Result<(), MigError> = Ok(());
        for cmd in cmds {
            if !self.cmd_table.contains_key(*cmd) {
                if let Ok(cmd_path) = whereis(cmd) {
                    self.cmd_table.insert(String::from(*cmd), cmd_path.clone());
                } else {
                    let message = format!("cannot find required command {}", cmd);
                    warn!("{}", message);
                    result = Err(MigError::from_remark(
                        MigErrorKind::NotFound,
                        &format!("{}", message),
                    ));
                }
            }
        }
        result
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

    #[allow(dead_code)]
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
