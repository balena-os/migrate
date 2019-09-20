use failure::ResultExt;
use log::{debug, error, info, warn};
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str;
use std::thread;
use std::time::{Duration, SystemTime};

use nix::unistd::sync;

use crate::common::file_exists;
use crate::common::stage2_config::CheckedFSDump;
use crate::{
    common::{
        call,
        config::balena_config::PartCheck,
        path_append,
        stage2_config::{CheckedImageType, Stage2Config},
        MigErrCtx, MigError, MigErrorKind,
    },
    defs::{DEF_BLOCK_SIZE, PART_NAME},
    linux::{
        linux_defs::{EXT_FMT_CMD, FAT_FMT_CMD, LSBLK_CMD, PARTED_CMD, PARTPROBE_CMD, TAR_CMD},
        stage2::{mounts::Mounts, FlashResult},
        lsblk_info::{LsblkInfo, LsblkDevice}
    },
};

// TODO: ensure support for GPT partition tables

const FORMAT_WITH_LABEL: bool = true;
const DEFAULT_PARTITION_ALIGNMENT_KIB: u64 = 4096; // KiB
                                                   // should we maximize data partition to fill disk
                                                   // TODO: true might be the better default but can be very slow in combination with mkfs_direct_io
const DEFAULT_MAX_DATA: bool = true;

// TODO: replace removed command checks ?
/*pub const REQUIRED_CMDS: &[&str] = &[
    EXT_FMT_CMD,
    FAT_FMT_CMD,
    TAR_CMD,
    LSBLK_CMD,
    PARTPROBE_CMD,
    // SFDISK_CMD,
    PARTED_CMD,
];*/

pub(crate) fn write_balena_os(
    device: &Path,
    mounts: &mut Mounts,
    config: &Stage2Config,
    base_path: &Path,
) -> FlashResult {
    // make sure we have allrequired commands
    if let CheckedImageType::FileSystems(ref fs_dump) = config.get_balena_image() {
        let res = partition(device, fs_dump);
        if let FlashResult::Ok = res {
            let lsblk_dev = match part_reread(device, 30, PART_NAME.len()) {
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

            if format(&lsblk_dev, fs_dump) {
                // TODO: need partprobe ?
                if let Err(why) = mounts.mount_balena(true) {
                    error!(
                        "write_balena_os: failed mount balena partitions, error: {:?}",
                        why
                    );
                    sync();
                    return FlashResult::FailNonRecoverable;
                }

                if balena_write(mounts, TAR_CMD, fs_dump, base_path) {
                    sync();
                    match call(
                        LSBLK_CMD,
                        &["-o", "name,partuuid", &device.to_string_lossy()],
                        true,
                    ) {
                        Ok(cmd_res) => {
                            if cmd_res.status.success() {
                                debug!("lsblk after fs-write: '{}'", cmd_res.stdout);
                            } else {
                                warn!("lsblk failure after fs-write: '{}'", cmd_res.stderr);
                            }
                        }
                        Err(why) => {
                            warn!("lsblk failure after fs-write, error {:?}", why);
                        }
                    }

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

fn sub_write(tar_path: &str, mountpoint: &Path, base_path: &Path, archive: &PathBuf) -> bool {
    let arch_path = path_append(base_path, archive);
    let tar_args: &[&str] = &[
        "-xzf",
        &arch_path.to_string_lossy(),
        "-C",
        &mountpoint.to_string_lossy(),
    ];

    debug!("sub_write: invoking '{}' with {:?}", tar_path, tar_args);

    match Command::new(tar_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .args(tar_args)
        .output()
    {
        Ok(cmd_res) => {
            if cmd_res.status.success() {
                info!(
                    "Successfully wrote '{}' to '{}'",
                    arch_path.display(),
                    mountpoint.display()
                );
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
}

fn balena_write(
    mounts: &Mounts,
    tar_path: &str,
    fs_dump: &CheckedFSDump,
    base_path: &Path,
) -> bool {
    // TODO: try use device labels instead

    debug!(
        "Attempting to write file systems, base path: '{}'",
        base_path.display()
    );

    if let Some(mountpoint) = mounts.get_balena_boot_mountpoint() {
        if !sub_write(
            tar_path,
            mountpoint,
            base_path,
            &fs_dump.boot.archive.rel_path,
        ) {
            return false;
        }
    } else {
        error!("Could not retrieve boot mountpoint");
        return false;
    }

    if let Some(mountpoint) = mounts.get_balena_root_a_mountpoint() {
        if !sub_write(
            tar_path,
            mountpoint,
            base_path,
            &fs_dump.root_a.archive.rel_path,
        ) {
            return false;
        }
    } else {
        error!("Could not retrieve root_a mountpoint");
        return false;
    }

    if let Some(mountpoint) = mounts.get_balena_root_b_mountpoint() {
        if !sub_write(
            tar_path,
            mountpoint,
            base_path,
            &fs_dump.root_b.archive.rel_path,
        ) {
            return false;
        }
    } else {
        error!("Could not retrieve root_b mountpoint");
        return false;
    }

    if let Some(mountpoint) = mounts.get_balena_state_mountpoint() {
        if !sub_write(
            tar_path,
            mountpoint,
            base_path,
            &fs_dump.state.archive.rel_path,
        ) {
            return false;
        }
    } else {
        error!("Could not retrieve state mountpoint");
        return false;
    }

    if let Some(mountpoint) = mounts.get_balena_data_mountpoint() {
        sub_write(
            tar_path,
            mountpoint,
            base_path,
            &fs_dump.data.archive.rel_path,
        )
    } else {
        error!("Could not retrieve data mountpoint");
        return false;
    }
}

fn sub_format(
    device: &Path,
    label: &str,
    is_fat: bool,
    check: &PartCheck,
    direct_io: bool,
) -> bool {
    debug!(
        "sub_format: entered with: '{}' is_fat: {}, check: {:?}",
        device.display(),
        is_fat,
        check
    );
    let dev_path = String::from(&*device.to_string_lossy());
    let mut args: Vec<&str> = Vec::new();

    let command = if is_fat {
        if FORMAT_WITH_LABEL {
            args.append(&mut vec!["-n", label]);
        }
        FAT_FMT_CMD
    } else {
        // TODO: sort this out. -O ^64bit is no good on big filesystems +16TB

        // TODO: Default opts for balena -E lazy_itable_init=0,lazy_journal_init=0 -i 8192 -v

        args.append(&mut vec![
            "-O",
            "^64bit",
            "-E",
            "lazy_itable_init=0,lazy_journal_init=0",
            "-i",
            "8192",
            "-v",
            "-F",
            "-F", // don't let anything get in our way
            // "-n",            // Pretend
            "-e",
            "remount-ro", // "continue" | "remount-ro" | "panic"
        ]); // Try remount-ro, anything but panic

        if direct_io {
            args.push("-D"); // Do direct I/O - very slow
        }

        if FORMAT_WITH_LABEL {
            args.append(&mut vec!["-L", label]);
        }

        EXT_FMT_CMD
    };

    match check {
        PartCheck::None => (),
        PartCheck::ReadOnly => {
            args.push("-c");
        }
        PartCheck::ReadWrite => {
            args.push("-cc");
        }
    }

    args.push(&dev_path);

    debug!("calling {} with args {:?}", command, args);
    sync();
    let cmd_res = match Command::new(command)
        .stdin(Stdio::inherit()) // test, test
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .args(&args)
        .output()
    {
        Ok(cmd_res) => cmd_res,
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
        info!("successfully formatted drive '{}'", dev_path);
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

fn format(lsblk_dev: &LsblkDevice, fs_dump: &CheckedFSDump) -> bool {
    if let Some(ref children) = lsblk_dev.children {
        // write an empty /etc/mtab to make mkfs happy
        let mtab_path = PathBuf::from("/etc/mtab");
        if !file_exists(&mtab_path) {
            match OpenOptions::new().create(true).write(true).open(&mtab_path) {
                Ok(_) => (),
                Err(why) => {
                    error!(
                        "failed to create an empty '{}', error: {:?}",
                        mtab_path.display(),
                        why
                    );
                }
            }
        }

        let check = if let Some(ref check) = fs_dump.check {
            check
        } else {
            // TODO: default to None until checks are supported in mke2fs ?
            &PartCheck::None
            // &PartCheck::Read
        };

        let fat_check = if let PartCheck::ReadWrite = check {
            // mkdosfs does not know about ReadWrite checks
            &PartCheck::ReadOnly
        } else {
            check
        };

        let mut dev_idx: usize = 0;
        let mut part_idx: usize = 0;

        while (part_idx < children.len()) && (part_idx < PART_NAME.len()) {
            if let Some(ref part_type) = children[dev_idx].parttype {
                match part_type.as_ref() {
                    "0xc" | "0xe" => {
                        debug!("Formatting fat partition at index {}/{}", dev_idx, part_idx);
                        if !sub_format(
                            &children[dev_idx].get_path(),
                            PART_NAME[part_idx],
                            true,
                            fat_check,
                            false,
                        ) {
                            return false;
                        } else {
                            part_idx += 1;
                        }
                    }
                    "0x83" => {
                        debug!(
                            "Formatting linux partition at index {}/{}",
                            dev_idx, part_idx
                        );
                        let direct_io = if let Some(mkfs_direct) = fs_dump.mkfs_direct {
                            mkfs_direct
                        } else {
                            false
                        };

                        if !sub_format(
                            &children[dev_idx].get_path(),
                            PART_NAME[part_idx],
                            false,
                            check,
                            direct_io,
                        ) {
                            return false;
                        } else {
                            part_idx += 1;
                        }
                    }
                    "0x5" | "0xf" => {
                        debug!(
                            "Skipping extended partition at index {}/{}",
                            dev_idx, part_idx
                        );
                    }
                    _ => {
                        error!(
                            "Invalid partition, type: {} found at index {}/{}",
                            part_type, dev_idx, part_idx
                        );
                        return false;
                    }
                }
            }

            dev_idx += 1;
        }

        if part_idx < PART_NAME.len() {
            error!(
                "format: not all partitions were formatted: {}/{}",
                part_idx,
                PART_NAME.len()
            );
            false
        } else {
            true
        }
    } else {
        error!("format: no partitions found to format");
        false
    }
}

fn partition(device: &Path, fs_dump: &CheckedFSDump) -> FlashResult {
    /*
    parted -s -a none /dev/sdb -- unit s \
    mklabel msdos \
    mkpart primary fat32 8192 90111 \
    set 1 boot on \
    mkpart primary ext2 90112 729087 \
    mkpart primary ext2 729088 1368063 \
    mkpart extended 1368064 3530751 \
    mkpart logical ext2 1376256 1417215 \
    mkpart logical ext2 1425408 3530751 \
    */

    let dev_name = String::from(&*device.to_string_lossy());

    let mut args: Vec<&str> = vec![
        "-s", "-a", "none", &dev_name, "--", "unit", "s", "mklabel", "msdos",
    ];

    // TODO: configure partition type

    let alignment_blocks: u64 = DEFAULT_PARTITION_ALIGNMENT_KIB * 1024 / DEF_BLOCK_SIZE as u64;
    debug!(
        "Alignment '{}'KiB, {} blocks",
        DEFAULT_PARTITION_ALIGNMENT_KIB, alignment_blocks
    );

    debug!(
        "Writing resin-boot as 'size={},bootable,type=e' to '{}'",
        fs_dump.boot.blocks,
        device.display()
    );

    let mut start_block: u64 = alignment_blocks;
    let end_block: u64 = start_block + fs_dump.boot.blocks;

    args.push("mkpart");
    args.push("primary");
    args.push("fat32");
    let p1_start = format!("{}", start_block);
    args.push(&p1_start);
    let p1_end = format!("{}", end_block - 1);
    args.push(&p1_end);

    args.push("set");
    args.push("1");
    args.push("boot");
    args.push("on");

    start_block = end_block;
    if (start_block % alignment_blocks) != 0 {
        start_block = (start_block / alignment_blocks + 1) * alignment_blocks;
    }

    let end_block: u64 = start_block + fs_dump.root_a.blocks;

    args.push("mkpart");
    args.push("primary");
    args.push("ext2");
    let p2_start = format!("{}", start_block);
    args.push(&p2_start);
    let p2_end = format!("{}", end_block - 1);
    args.push(&p2_end);

    start_block = end_block;
    if (start_block % alignment_blocks) != 0 {
        start_block = (start_block / alignment_blocks + 1) * alignment_blocks;
    }
    let end_block: u64 = start_block + fs_dump.root_b.blocks;

    args.push("mkpart");
    args.push("primary");
    args.push("ext2");
    let p3_start = format!("{}", start_block);
    args.push(&p3_start);
    let p3_end = format!("{}", end_block - 1);
    args.push(&p3_end);

    start_block = end_block;
    if (start_block % alignment_blocks) != 0 {
        start_block = (start_block / alignment_blocks + 1) * alignment_blocks;
    }

    let max_data = if let Some(max_data) = fs_dump.max_data {
        max_data
    } else {
        DEFAULT_MAX_DATA
    };

    // TODO: make ext part type configurable
    args.push("mkpart");
    args.push("extended");
    let p4_start = format!("{}", start_block);
    args.push(&p4_start);

    let p4_end = if max_data {
        String::from("-1")
    } else {
        format!("{}", start_block + fs_dump.extended_blocks - 1)
    };

    args.push(&p4_end);

    start_block += alignment_blocks;
    let end_block: u64 = start_block + fs_dump.state.blocks;

    args.push("mkpart");
    args.push("logical");
    args.push("ext2");
    let p5_start = format!("{}", start_block);
    args.push(&p5_start);
    let p5_end = format!("{}", end_block - 1);
    args.push(&p5_end);

    // in dos extended partition at least 1 block offset is needed for the next extended entry
    // so align to next and add an extra alignment block
    start_block = end_block;
    if (start_block % alignment_blocks) != 0 {
        // TODO: clarify if this is right
        start_block = (start_block / alignment_blocks + 2) * alignment_blocks;
    } else {
        start_block += alignment_blocks;
    }

    args.push("mkpart");
    args.push("logical");
    args.push("ext2");
    let p6_start = format!("{}", start_block);
    args.push(&p6_start);
    let p6_end = if max_data {
        String::from("-1")
    } else {
        format!("{}", start_block + fs_dump.data.blocks - 1)
    };
    args.push(&p6_end);

    debug!("using parted with args: {:?}", args);

    match call(PARTED_CMD, &args, true) {
        Ok(cmd_res) => {
            if !cmd_res.status.success() {
                error!(
                    "parted returned an error status: code: {:?}, stderr: {:?}",
                    cmd_res.status.code(),
                    cmd_res.stderr
                );
                FlashResult::FailNonRecoverable
            } else {
                sync();
                debug!("parted stdout: {:?}", cmd_res.stdout);
                FlashResult::Ok
            }
        }
        Err(why) => {
            error!(
                "Failed to run command : '{}' with args: {:?}, error: {:?}",
                PARTED_CMD, args, why
            );
            FlashResult::FailRecoverable
        }
    }
}

fn part_reread(
    device: &Path,
    timeout: u64,
    num_partitions: usize,
) -> Result<LsblkDevice, MigError> {
    debug!(
        "part_reread: entered with: '{}', timeout: {}, num_partitions: {}",
        device.display(),
        timeout,
        num_partitions
    );

    let start = SystemTime::now();
    thread::sleep(Duration::from_secs(1));

    match call(PARTPROBE_CMD, &[&device.to_string_lossy()], true) {
        Ok(cmd_res) => {
            if !cmd_res.status.success() {
                warn!(
                    "part_reread: partprobe returned: stderr: '{}'",
                    cmd_res.stderr
                );
            } else {
                debug!(
                    "part_reread: partprobe returned: stdout '{}'",
                    cmd_res.stdout
                );
            }
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
        let lsblk_dev = LsblkInfo::for_device(device)?;
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
