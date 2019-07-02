use failure::ResultExt;
use log::{debug, error, warn};
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::{ChildStdin, Command, Stdio};
use std::str;
use std::thread;
use std::time::Duration;

use crate::{
    common::{
        config::balena_config::{FSDump, PartCheck},
        path_append,
        stage2_config::{CheckedImageType, Stage2Config},
        MigErrCtx, MigError, MigErrorKind,
    },
    defs::{PART_FSTYPE, PART_NAME},
    linux::{
        ensured_cmds::{
            EnsuredCmds, EXT_FMT_CMD, FAT_FMT_CMD, LSBLK_CMD, PARTPROBE_CMD, SFDISK_CMD, TAR_CMD,
        },
        extract::Partition,
        linux_defs::PRE_PARTPROBE_WAIT_SECS,
        migrate_info::{LsblkDevice, LsblkInfo},
        stage2::{mounts::Mounts, FlashResult},
    },
};

pub const REQUIRED_CMDS: &[&str] = &[
    SFDISK_CMD,
    EXT_FMT_CMD,
    FAT_FMT_CMD,
    TAR_CMD,
    LSBLK_CMD,
    PARTPROBE_CMD,
];

// TODO: partition & untar balena to drive

pub(crate) fn write_balena_os(
    device: &Path,
    cmds: &EnsuredCmds,
    mounts: &mut Mounts,
    config: &Stage2Config,
    base_path: &Path,
) -> FlashResult {
    // make sure we have allrequired commands
    if let CheckedImageType::FileSystems(ref fs_dump) = config.get_balena_image().image {
        let res = partition(device, cmds.get(SFDISK_CMD).unwrap(), fs_dump);

        if let FlashResult::Ok = res {
            let lsblk_dev = match LsblkInfo::for_device(device, cmds) {
                Ok(lsblk_dev) => lsblk_dev,
                Err(why) => {
                    error!(
                        "write_balena_os: failed get updated device info (2), error: {:?}",
                        why
                    );
                    return FlashResult::FailNonRecoverable;
                }
            };

            if format(
                &lsblk_dev,
                cmds.get(FAT_FMT_CMD).unwrap(),
                cmds.get(EXT_FMT_CMD).unwrap(),
                fs_dump,
            ) {
                // TODO: need partprobe ?

                thread::sleep(Duration::from_secs(PRE_PARTPROBE_WAIT_SECS));

                if let Err(why) = cmds.call(
                    PARTPROBE_CMD,
                    &[&lsblk_dev.get_path().to_string_lossy()],
                    true,
                ) {
                    warn!(
                        "write_balena_os: partprobe command failed, ignoring,  error: {:?}",
                        why
                    );
                }

                if let Err(why) = mounts.mount_balena(true) {
                    error!(
                        "write_balena_os: failed mount balena partitions, error: {:?}",
                        why
                    );
                    return FlashResult::FailNonRecoverable;
                }

                if balena_write(mounts, cmds.get(TAR_CMD).unwrap(), fs_dump, base_path) {
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
    if let Some(archive) = archive {
        let arch_path = path_append(base_path, archive);
        let tar_args: &[&str] = &[
            "-xzf",
            &arch_path.to_string_lossy(),
            "-C",
            &device.to_string_lossy(),
        ];

        match Command::new(tar_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .args(tar_args)
            .output()
        {
            Ok(cmd_res) => {
                if cmd_res.status.success() {
                    true
                } else {
                    error!(
                        "sub_write: failed to untar archive with {} {:?}, code: {:?} stderr: {:?}",
                        tar_path,
                        tar_args,
                        cmd_res.status.code(),
                        str::from_utf8(&cmd_res.stderr)
                    );
                    false
                }
            }
            Err(why) => {
                error!(
                    "sub_write: failed to untar archive with {} {:?}, error: {:?}",
                    tar_path, tar_args, why
                );
                false
            }
        }
    } else {
        error!("sub_write: a required archive was not found",);
        false
    }
}

fn balena_write(mounts: &Mounts, tar_path: &str, fs_dump: &FSDump, base_path: &Path) -> bool {
    // TODO: try use device labels instead

    if let Some(mountpoint) = mounts.get_balena_boot_mountpoint() {
        if !sub_write(tar_path, mountpoint, base_path, &fs_dump.boot.archive) {
            return false;
        }
    } else {
        error!("Could not retrieve boot mountpoint");
        return false;
    }

    if let Some(mountpoint) = mounts.get_balena_root_a_mountpoint() {
        if !sub_write(tar_path, mountpoint, base_path, &fs_dump.root_a.archive) {
            return false;
        }
    } else {
        error!("Could not retrieve root_a mountpoint");
        return false;
    }

    if let Some(ref _archive) = fs_dump.root_b.archive {
        if let Some(mountpoint) = mounts.get_balena_root_b_mountpoint() {
            if !sub_write(tar_path, mountpoint, base_path, &fs_dump.root_b.archive) {
                return false;
            }
        } else {
            error!("Could not retrieve root_b mountpoint");
            return false;
        }
    }

    if let Some(ref _archive) = fs_dump.state.archive {
        if let Some(mountpoint) = mounts.get_balena_state_mountpoint() {
            if !sub_write(tar_path, mountpoint, base_path, &fs_dump.state.archive) {
                return false;
            }
        } else {
            error!("Could not retrieve state mountpoint");
            return false;
        }
    }

    if let Some(mountpoint) = mounts.get_balena_data_mountpoint() {
        sub_write(tar_path, mountpoint, base_path, &fs_dump.data.archive)
    } else {
        error!("Could not retrieve data mountpoint");
        return false;
    }
}

fn sub_format(device: &Path, label: &str, command: &str, check: &PartCheck) -> bool {
    let mut args: Vec<&str> = vec!["-n", label];
    match check {
        PartCheck::None => (),
        PartCheck::Read => {
            args.push("-c");
        }
        PartCheck::ReadWrite => {
            args.push("-cc");
        }
    }
    let dev_path = String::from(&*device.to_string_lossy());
    args.push(&dev_path);

    debug!("calling {} with args {:?}", command, args);
    let cmd_res = match Command::new(command).args(args).output() {
        Ok(cmd_res) => cmd_res,
        Err(why) => {
            error!(
                "format: failed to format drive with {}: '{}', error: {:?}",
                command, dev_path, why
            );
            return false;
        }
    };

    if cmd_res.status.success() {
        true
    } else {
        error!(
            "format: failed to format drive with {}: '{}', code: {:?}, stderr: {:?}",
            command,
            dev_path,
            cmd_res.status.code(),
            str::from_utf8(&cmd_res.stderr)
        );
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

            if !sub_format(
                &children[0].get_path(),
                PART_NAME[0],
                fat_fmt_path,
                fat_check,
            ) {
                return false;
            }

            if !sub_format(&children[1].get_path(), PART_NAME[1], ext_fmt_path, check) {
                return false;
            }

            if !sub_format(&children[2].get_path(), PART_NAME[2], ext_fmt_path, check) {
                return false;
            }

            if !sub_format(&children[4].get_path(), PART_NAME[3], ext_fmt_path, check) {
                return false;
            }

            sub_format(&children[5].get_path(), PART_NAME[4], ext_fmt_path, &check)
        } else {
            error!(
                "format: encountered an in valid number of partitions {} != 6",
                children.len()
            );
            false
        }
    } else {
        error!("format: no partitions found to format");
        false
    }
}

fn partition(device: &Path, sfdisk_path: &str, fs_dump: &FSDump) -> FlashResult {
    let mut sfdisk_cmd = match Command::new(sfdisk_path)
        .args(&["-f", &*device.to_string_lossy()])
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(why) => {
            error!(
                "Failed to start command : '{}', error: {:?}",
                sfdisk_path, why
            );
            return FlashResult::FailRecoverable;
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
                        error!(
                            "Failed to write some bytes to command stdin: {}  != {}",
                            bytes_written, count
                        );
                        return FlashResult::FailNonRecoverable;
                    }
                }
                Err(why) => {
                    error!("Failed to write to command stdin, error: {:?}", why);
                    return FlashResult::FailNonRecoverable;
                }
            }
        } else {
            error!("partition: sfdisk stdin could not be found");
            return FlashResult::FailRecoverable;
        }
    }

    debug!("done writing to sfdisk stdin - command should terminate now");

    // TODO: wait with timeout, terminate

    let cmd_res = match sfdisk_cmd.wait_with_output() {
        Ok(cmd_res) => cmd_res,
        Err(why) => {
            error!("failure waiting for sfdisk to terminate, error: {:?}", why);
            return FlashResult::FailNonRecoverable;
        }
    };

    if !cmd_res.status.success() {
        error!(
            "sfdisk returned an error status: code: {:?}, stderr: {:?}",
            cmd_res.status.code(),
            str::from_utf8(&cmd_res.stderr)
        );
        return FlashResult::FailNonRecoverable;
    }

    debug!("sfdisk stdout: {:?}", str::from_utf8(&cmd_res.stdout));
    FlashResult::Ok
}
