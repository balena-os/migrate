use failure::ResultExt;
use log::{debug, error, warn, info};
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str;
use std::thread;
use std::time::{Duration, SystemTime};

use nix::unistd::sync;

use crate::{
    common::{
        config::balena_config::{FSDump, PartCheck},
        path_append,
        stage2_config::{CheckedImageType, Stage2Config},
        MigErrCtx, MigError, MigErrorKind,
    },
    defs::PART_NAME,
    linux::{
        ensured_cmds::{
            EnsuredCmds, EXT_FMT_CMD, FAT_FMT_CMD, LSBLK_CMD, PARTPROBE_CMD, SFDISK_CMD, TAR_CMD,
        },
        migrate_info::{LsblkDevice, LsblkInfo},
        stage2::{mounts::Mounts, FlashResult},
    },
};

// TODO: ensure support for GPT partition tables

// pub const OPTIONAL_CMDS: &[&str] = &[SFDISK_CMD, FDISK_CMD];
pub const REQUIRED_CMDS: &[&str] = &[
    EXT_FMT_CMD,
    FAT_FMT_CMD,
    TAR_CMD,
    LSBLK_CMD,
    PARTPROBE_CMD,
    SFDISK_CMD,
];

pub(crate) fn check_commands(cmds: &mut EnsuredCmds) -> Result<(), MigError> {
    Ok(cmds.ensure_cmds(REQUIRED_CMDS)?)
}

pub(crate) fn write_balena_os(
    device: &Path,
    cmds: &EnsuredCmds,
    mounts: &mut Mounts,
    config: &Stage2Config,
    base_path: &Path,
) -> FlashResult {
    // make sure we have allrequired commands
    if let CheckedImageType::FileSystems(ref fs_dump) = config.get_balena_image().image {
        let res = if let Ok(command) = cmds.get(SFDISK_CMD) {
            sfdisk_part(device, command, fs_dump)
        } else {
            error!("write_balena_os: no partitioning command was found",);
            return FlashResult::FailRecoverable;
        };

        sync();

        if let FlashResult::Ok = res {
            let lsblk_dev = match part_reread(device, 30, PART_NAME.len(), cmds) {
                Ok(lsblk_device) => lsblk_device,
                Err(why) => {
                    error!(
                        "write_balena_os: The newly written partitions on '{}' did not show up as expected , error: {:?}",
                        device.display(),why
                    );
                    return FlashResult::FailNonRecoverable;
                }
            };

            sync();

            if format(&lsblk_dev, cmds, fs_dump) {
                // TODO: need partprobe ?
                if let Err(why) = mounts.mount_balena(true) {
                    error!(
                        "write_balena_os: failed mount balena partitions, error: {:?}",
                        why
                    );
                    sync();
                    return FlashResult::FailNonRecoverable;
                }

                if balena_write(mounts, cmds.get(TAR_CMD).unwrap(), fs_dump, base_path) {
                    FlashResult::Ok
                } else {
                    error!("write_balena_os: failed initialise devices");
                    sync();
                    FlashResult::FailNonRecoverable
                }
            } else {
                error!("write_balena_os: failed to format devices");
                sync();
                FlashResult::FailNonRecoverable
            }
        } else {
            error!("write_balena_os: failed to partition device");
            sync();
            res
        }
    } else {
        error!("write_balena_os: encountered invalid image type");
        sync();
        FlashResult::FailRecoverable
    }
}

fn sub_write(
    tar_path: &str,
    mountpoint: &Path,
    base_path: &Path,
    archive: &Option<PathBuf>,
) -> bool {
    if let Some(archive) = archive {
        let arch_path = path_append(base_path, archive);
        let tar_args: &[&str] = &[
            "-xzf",
            &arch_path.to_string_lossy(),
            "-C",
            &mountpoint.to_string_lossy(),
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

fn sub_format(
    device: &Path,
    label: &str,
    cmds: &EnsuredCmds,
    is_fat: bool,
    check: &PartCheck,
) -> bool {

    debug!("sub_format: entered with: '{}' is_fat: {}, check: {:?}", device.display(), is_fat, check);
    let dev_path = String::from(&*device.to_string_lossy());
    let mut args: Vec<&str> = Vec::new();

    let command = if is_fat {
        args.append(&mut vec!["-n", label]);
        match cmds.get(FAT_FMT_CMD) {
            Ok(command) => command,
            Err(why) => {
                error!(
                    "format: the format command was not found  {}, error: {:?}",
                    FAT_FMT_CMD, why
                );
                return false;
            }
        }
    } else {
        // TODO: sort this out. -O ^64bit is no good on big filesystems +16TB
        args.append(&mut vec!["-O", "^64bit", "-F", "-L", label]);
        match cmds.get(EXT_FMT_CMD) {
            Ok(command) => command,
            Err(why) => {
                error!(
                    "format: the format command was not found  {}, error: {:?}",
                    EXT_FMT_CMD, why
                );
                return false;
            }
        }
    };

    match check {
        PartCheck::None => (),
        PartCheck::Read => {
            args.push("-c");
        }
        PartCheck::ReadWrite => {
            args.push("-cc");
        }
    }

    args.push(&dev_path);

    debug!("calling {} with args {:?}", command, args);
    sync();
    let cmd_res = match Command::new(command).args(&args).output() {
        Ok(cmd_res) => {
            cmd_res
        },
        Err(why) => {
            error!(
                "format: failed to format drive with {}: '{}', error: {:?}",
                command, dev_path, why
            );
            sync();
            return false;
        }
    };

    if cmd_res.status.success() {
        info!(
            "successfully formatted drive '{}'",
            dev_path
        );
        sync();
        true
    } else {
        error!(
            "sub_format: failed to format drive with {}: '{:?}', code: {:?}, stderr: {:?}",
            command,
            args,
            cmd_res.status.code(),
            str::from_utf8(&cmd_res.stderr)
        );
        false
    }
}

fn format(lsblk_dev: &LsblkDevice, cmds: &EnsuredCmds, fs_dump: &FSDump) -> bool {
    if let Some(ref children) = lsblk_dev.children {

        let check = if let Some(ref check) = fs_dump.check {
            check
        } else {
            // TODO: default to None until checks are supported in mke2fs ?
            &PartCheck::None
            // &PartCheck::Read
        };

        let fat_check = if let PartCheck::ReadWrite = check {
            // mkdosfs does not know about ReadWrite checks
            &PartCheck::Read
        } else {
            check
        };

        let mut dev_idx: usize  = 0;
        let mut part_idx: usize  = 0;

        while (part_idx < children.len()) && (part_idx < PART_NAME.len()) {
            if let Some(ref part_type) = children[dev_idx].parttype {
                match part_type.as_ref() {
                    "0xe" => {
                        debug!("Formatting fat partition at index {}/{}", dev_idx, part_idx);
                        if !sub_format(&children[dev_idx].get_path(), PART_NAME[part_idx], cmds, true, fat_check) {
                            return false;
                        } else {
                            part_idx += 1;
                        }
                    },
                    "0x83" => {
                        debug!("Formatting linux partition at index {}/{}", dev_idx, part_idx);
                        if !sub_format(&children[dev_idx].get_path(), PART_NAME[part_idx], cmds, false, check) {
                            return false;
                        } else {
                            part_idx += 1;
                        }
                    },
                    "0x5"|"0xf" => {
                        debug!("Skipping extended partition at index {}/{}", dev_idx, part_idx);
                    },
                    _ => {
                        error!("Invalid partition, type: {} found at index {}/{}", part_type, dev_idx, part_idx);
                        return false;
                    }
                }
            }

            dev_idx += 1;
        }

        if part_idx < PART_NAME.len() {
            error!("format: not all partitions were formatted: {}/{}", part_idx, PART_NAME.len());
            false
        } else {
            true
        }
    } else {
        error!("format: no partitions found to format");
        false
    }
}

fn sfdisk_part(device: &Path, sfdisk_path: &str, fs_dump: &FSDump) -> FlashResult {
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

            debug!("Writing resin-state as 'type=83' to '{}'", device.display());
            buffer.push_str(&format!("type=83\n"));

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

fn part_reread(
    device: &Path,
    timeout: u64,
    num_partitions: usize,
    cmds: &EnsuredCmds,
) -> Result<LsblkDevice, MigError> {
    debug!(
        "part_reread: entered with: '{}', timeout: {}, num_partitions: {}",
        device.display(),
        timeout,
        num_partitions
    );

    let start = SystemTime::now();
    thread::sleep(Duration::from_secs(1));

    match cmds.call(PARTPROBE_CMD, &[&device.to_string_lossy()], true) {
        Ok(cmd_res) => {
            debug!(
                "part_reread: partprobe returned: stdout '{}', stderr: '{}'",
                cmd_res.stdout, cmd_res.stderr
            );
        }
        Err(why) => {
            warn!(
                "write_balena_os: partprobe command failed, ignoring,  error: {:?}",
                why
            );
        }
    }

    loop {
        thread::sleep(Duration::from_secs(1));
        debug!(
            "part_reread: calling LsblkInfo::for_device('{}')",
            device.display()
        );
        let lsblk_dev = LsblkInfo::for_device(device, cmds)?;
        if let Some(children) = &lsblk_dev.children {
            if children.len() >= num_partitions {
                debug!(
                    "part_reread: LsblkInfo::for_device('{}') : {:?}",
                    device.display(),
                    lsblk_dev
                );
                return Ok(lsblk_dev);
            } else {
                debug!(
                    "part_reread: not accepting LsblkInfo::for_device('{}') : {:?}",
                    device.display(),
                    lsblk_dev
                );
            }
        } else {
            debug!(
                "part_reread: not accepting LsblkInfo::for_device('{}') : {:?}",
                device.display(),
                lsblk_dev
            );
        }

        let elapsed = SystemTime::now()
            .duration_since(start)
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                "Failed to conpute elapsed time",
            ))?;

        if elapsed.as_secs() > timeout {
            return Err(MigError::from_remark(
                MigErrorKind::Timeout,
                &format!(
                    "The partitioned devices did not show up as expected after {} secs",
                    elapsed.as_secs()
                ),
            ));
        }
    }
}
