use failure::ResultExt;
use log::{debug, error};
use std::path::Path;
use std::process::{Command, Stdio, ChildStdin};
use std::io::Write;
use std::path::{PathBuf};
use std::str;

use crate::{
    common::{
        stage2_config::{CheckedImageType, Stage2Config},
        MigErrCtx, MigError, MigErrorKind,
    },
    defs::{PART_NAME, PART_FSTYPE},
    linux::{
        ensured_cmds::{EnsuredCmds, SFDISK_CMD, FAT_FMT_CMD, EXT_FMT_CMD, TAR_CMD, LSBLK_CMD},
        extract::Partition,
        stage2::FlashResult,
        migrate_info::LsblkInfo
    },
};
use crate::common::config::balena_config::FSDump;
use crate::linux::migrate_info::LsblkDevice;


// TODO: partition & untar balena to drive

pub(crate) fn write_balena_os(
    device: &Path,
    cmds: &mut EnsuredCmds,
    config: &Stage2Config
) -> FlashResult {
    // make sure we have allrequired commands
    match cmds.ensure_cmds(&[SFDISK_CMD, EXT_FMT_CMD, FAT_FMT_CMD, TAR_CMD, LSBLK_CMD ]) {
        Ok(_) => (),
        Err(why) => {
            error!("Failed to ensure commands, error: {:?}", why);
            return FlashResult::FailRecoverable
        }
    }

    if let CheckedImageType::FileSystems(ref fs_dump) = config.get_balena_image().image {
        let res = partition(device, cmds.get(SFDISK_CMD).unwrap(), fs_dump);
        if let FlashResult::Ok = res {
           let lsblk_dev =  match LsblkInfo::for_device(device, cmds) {
               Ok(lsblk_info) => {
                   if let Some(lsblk_dev) = lsblk_info.get_blk_devices().get(0) {
                       lsblk_dev
                   } else {
                       error!("write_balena_os: failed get updated device info (1), error: {:?}");
                       FlashResult::FailNonRecoverable
                   }
               },
               Err(why) => {
                   error!("write_balena_os: failed get updated device info (2), error: {:?}");
                   FlashResult::FailNonRecoverable
               },
           };

            if format(lsblk_dev, cmds.get(FAT_FMT_CMD).unwrap(), cmds.get(EXT_FMT_CMD).unwrap(), fs_dump) {
                if balena_write(device, cmds.get(TAR_CMD).unwrap(), fs_dump) {
                    FlashResult::Ok
                } else {
                    error!("write_balena_os: failed initialise devices");
                    FlashResult::FailNonRecoverable
                }
            } else {
                error!("write_balena_os: failed to format devices");
                FlashResult::FailNonRecoverable
            }
        } else {
            error!("write_balena_os: failed to partition device");
            res
        }
    } else {
        error!("write_balena_os: encountered invalid image type");
        FlashResult::FailRecoverable
    }
}

fn balena_write(
    device: &Path,
    tar_path: &str,
    fs_dump: &FSDump,
) -> bool {
    true
}

fn format(
    lsblk_info: &LsblkDevice,
    fat_fmt_path: &str,
    ext_fmt_path: &str,
    fs_dump: &FSDump,
) -> bool {

    true
}


fn partition(
    device: &Path,
    sfdisk_path: &str,
    fs_dump: &FSDump,
) -> FlashResult {
    let mut sfdisk_cmd = match Command::new(sfdisk_path)
        .args(&["-f", &*device.to_string_lossy()])
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .stdin(Stdio::piped())
        .spawn() {
        Ok(child) => child,
        Err(why) => {
            error!("Failed to start command : '{}', error: {:?}", sfdisk_path, why);
            return FlashResult::FailRecoverable
        }
    };

    // TODO: configure partition type

    {
        if let Some(ref mut stdin) = sfdisk_cmd.stdin {
            debug!("Writing a new partition table to '{}'", device.display());
            let mut buffer: String = String::from("label: dos\n");

            debug!(
                "Writing resin-boot as 'size={},bootable,type=e' to '{}'",
                fs_dump.boot.blocks,
                device.display()
            );

            buffer.push_str(&format!("size={},bootable,type=e\n", fs_dump.boot.blocks));


            debug!(
                "Writing resin-rootA as 'size={},type=83' to '{}'",
                fs_dump.root_a.blocks,
                device.display()
            );
            buffer.push_str(&format!("size={},type=83\n", fs_dump.root_a.blocks));

            debug!(
                "Writing resin-rootB as 'size={},type=83' to '{}'",
                fs_dump.root_b.blocks,
                device.display()
            );
            buffer.push_str(&format!("size={},type=83\n", fs_dump.root_b.blocks));

            // extended partition
            debug!(
                "Writing extended partition as 'type=5' to '{}'",
                device.display()
            );
            buffer.push_str("type=5\n");

            debug!(
                "Writing resin-state as 'size={},type=83' to '{}'",
                fs_dump.state.blocks,
                device.display()
            );

            buffer.push_str(&format!("size={},type=83\n", fs_dump.state.blocks));

            debug!(
                "Writing resin-state as 'size={},type=83' to '{}'",
                fs_dump.state.blocks,
                device.display()
            );
            buffer.push_str(&format!("size={},type=83\n", fs_dump.data.blocks));


            debug!("writing partitioning as: \n{}", buffer);

            let data = buffer.as_bytes();
            let count = data.len();
            match stdin.write(data) {
                Ok(bytes_written) => {
                    if bytes_written != count {
                        error!("Failed to write some bytes to command stdin: {}  != {}", bytes_written, count);
                        return FlashResult::FailNonRecoverable
                    }
                },
                Err(why) => {
                    error!("Failed to write to command stdin, error: {:?}", why);
                    return FlashResult::FailNonRecoverable
                }
            }
        } else {
            error!("partition: sfdisk stdin could not be found");
            return FlashResult::FailRecoverable;
        }
    }

    debug!("done writing to sfdisk stdin - command should terminate now");

    // TODO: wait with timeout, terminate

    let cmd_res = match sfdisk_cmd
        .wait_with_output() {
        Ok(cmd_res) => cmd_res,
        Err(why) => {
            error!("failure waiting for sfdisk to terminate, error: {:?}", why);
            return FlashResult::FailNonRecoverable
        }
    };

    if !cmd_res.status.success() {
        error!("sfdisk returned an error status: code: {:?}, stderr: {:?}", cmd_res.status.code(), str::from_utf8(&cmd_res.stderr));
        return FlashResult::FailNonRecoverable
    }

    debug!("sfdisk stdout: {:?}", str::from_utf8(&cmd_res.stdout));
    FlashResult::Ok
}
