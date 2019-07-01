use failure::ResultExt;
use log::{debug, error};
use std::path::Path;
use std::process::{Command, Stdio, ChildStdin};
use std::io::Write;
use std::path::{PathBuf};
use std::str;

use crate::{
    common::{
        config::balena_config::{PartCheck},
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
use crate::common::path_append;


// TODO: partition & untar balena to drive

pub(crate) fn write_balena_os(
    device: &Path,
    cmds: &mut EnsuredCmds,
    config: &Stage2Config,
    base_path: &Path,
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
            let lsblk_dev = match LsblkInfo::for_device(device, cmds) {
                Ok(lsblk_dev) => lsblk_dev,
                Err(why)=> {
                    error!("write_balena_os: failed get updated device info (2), error: {:?}", why);
                    return FlashResult::FailNonRecoverable;
                }
            };

            if format(&lsblk_dev, cmds.get(FAT_FMT_CMD).unwrap(), cmds.get(EXT_FMT_CMD).unwrap(), fs_dump) {
                if balena_write(&lsblk_dev, cmds.get(TAR_CMD).unwrap(), fs_dump, base_path) {
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

fn sub_write(tar_path: &str, device: &Path, base_path: &Path, archive: &Option<PathBuf>) -> bool {
    let tar_args: &[&str] = &[];
    match Command::new(tar_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(tar_args)
        .spawn() {
        Ok(cmd_res) => {
            true
        },
        Err(why) => {
            error!("format: failed to untar archive with {} {:?}, error: {:?}", tar_path, tar_args, why);
            false
        }
    }
}

fn balena_write(
    lsblk_dev: &LsblkDevice,
    tar_path: &str,
    fs_dump: &FSDump,
    base_path: &Path,
) -> bool {
    // TODO: try use device labels instead

    if let Some(ref children) = lsblk_dev.children {
        if children.len() == 6 {
            if ! sub_write(tar_path, &children[0].get_path(), base_path, &fs_dump.boot.archive) {
                return false
            }

            if ! sub_write(tar_path, &children[1].get_path(), base_path, &fs_dump.root_a.archive) {
                return false
            }

            if let Some(ref _archive) = fs_dump.root_b.archive {
                if ! sub_write(tar_path, &children[2].get_path(), base_path, &fs_dump.root_b.archive) {
                    return false
                }
            }

            if let Some(ref _archive) = fs_dump.state.archive {
                if ! sub_write(tar_path, &children[4].get_path(), base_path, &fs_dump.state.archive) {
                    return false
                }
            }

            sub_write(tar_path, &children[5].get_path(), base_path, &fs_dump.data.archive)
        } else {
            error!("balena_write: encountered an in valid number of partitions {} != 6", children.len());
            false
        }
    } else {
        error!("balena_write: no partitions found to format");
        false
    }
}

fn sub_format(device: &Path, label: &str, command: &str, check: &PartCheck) -> bool {
    let mut args: Vec<&str> = vec!["-n", label];
    match check {
        PartCheck::None => (),
        PartCheck::Read => {
            args.push("-c");
        },
        PartCheck::ReadWrite => {
            args.push("-cc");
        }
    }
    let dev_path = String::from(&*device.to_string_lossy());
    args.push(&dev_path);

    debug!("calling {} with args {:?}", command, args);
    let cmd_res = match Command::new(command)
        .args(args)
        .output() {
        Ok(cmd_res) => cmd_res,
        Err(why) => {
            error!("format: failed to format drive with {}: '{}', error: {:?}", command, dev_path, why);
            return false;
        }
    };

    if cmd_res.status.success() {
        true
    } else {
        error!("format: failed to format drive with {}: '{}', code: {:?}, stderr: {:?}",
               command, dev_path, cmd_res.status.code(), str::from_utf8(&cmd_res.stderr));
        false
    }
}


fn format(
    lsblk_dev: &LsblkDevice,
    fat_fmt_path: &str,
    ext_fmt_path: &str,
    fs_dump: &FSDump,
) -> bool {
    if let Some(ref children) = lsblk_dev.children {
      if children.len() == 6 {
          let check = if let Some(ref check) = fs_dump.check {
              check
          } else {
              // TODO: default to read check ?
              &PartCheck::Read
          };

          let fat_check = if let PartCheck::ReadWrite = check {
              // mkdosfs does not know about ReadWrite checks
              &PartCheck::Read
          } else {
              check
          };

          if ! sub_format(&children[0].get_path(), PART_NAME[0], fat_fmt_path, fat_check) {
              return false
          }

          if ! sub_format(&children[1].get_path(), PART_NAME[1], ext_fmt_path, check) {
              return false
          }

          if ! sub_format(&children[2].get_path(), PART_NAME[2], ext_fmt_path, check) {
              return false
          }

          if ! sub_format(&children[4].get_path(), PART_NAME[3], ext_fmt_path, check) {
              return false
          }

          sub_format(&children[5].get_path(), PART_NAME[4], ext_fmt_path, &check)
      } else {
          error!("format: encountered an in valid number of partitions {} != 6", children.len());
          false
      }
    } else {
        error!("format: no partitions found to format");
        false
    }
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
