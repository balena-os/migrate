use failure::ResultExt;
use log::{debug, error, info, warn};
use nix::unistd::sync;
use regex::Regex;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str;
use std::thread;
use std::time::{Duration, SystemTime};

use crate::defs::PART_FSTYPE;
use crate::{
    common::{
        call, call_with_stdin,
        config::balena_config::PartCheck,
        file_exists, path_append,
        stage2_config::{CheckedFSDump, CheckedImageType, Stage2Config},
        MigErrCtx, MigError, MigErrorKind,
    },
    defs::{DEF_BLOCK_SIZE, PART_NAME},
    linux::{
        linux_common::whereis,
        linux_defs::{EXT_FMT_CMD, FAT_FMT_CMD, LSBLK_CMD, PARTPROBE_CMD, SFDISK_CMD, TAR_CMD},
        lsblk_info::block_device::BlockDevice,
        stage2::{mounts::Mounts, FlashResult},
    },
};

// TODO: ensure support for GPT partition tables
// TODO: write tests for partitioning

const FORMAT_WITH_LABEL: bool = true;
const DEFAULT_PARTITION_ALIGNMENT_KIB: u64 = 4096; // KiB
                                                   // should we maximize data partition to fill disk
                                                   // TODO: true might be the better default but can be very slow in combination with mkfs_direct_io
const DEFAULT_MAX_DATA: bool = true;

// TODO: replace removed command checks ?

pub(crate) fn write_balena_os(
    device: &Path,
    mounts: &mut Mounts,
    config: &Stage2Config,
    base_path: &Path,
) -> FlashResult {
    // make sure we have allrequired commands
    let mut cmd_path: HashMap<&str, String> = HashMap::new();

    let result = [EXT_FMT_CMD, FAT_FMT_CMD, LSBLK_CMD, SFDISK_CMD, TAR_CMD]
        .iter()
        .all(|cmd| match whereis(*cmd) {
            Ok(path) => {
                cmd_path.insert(cmd, path);
                true
            }
            Err(why) => {
                error!(
                    "write_balena_os: Could not find {} executable: {:?}",
                    cmd, why
                );
                false
            }
        });

    // optional
    match whereis(PARTPROBE_CMD) {
        Ok(path) => {
            cmd_path.insert(PARTPROBE_CMD, path);
        }
        Err(why) => {
            error!(
                "write_balena_os: Could not find {} executable: {:?}, tolerating it",
                PARTPROBE_CMD, why
            );
        }
    }

    if !result {
        return FlashResult::FailRecoverable;
    }

    if let CheckedImageType::FileSystems(ref fs_dump) = config.get_balena_image() {
        let res = partition_sfdisk(device, fs_dump, &cmd_path);
        if let FlashResult::Ok = res {
            let lsblk_dev = match part_reread(device, 30, PART_NAME.len(), &cmd_path) {
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

            if format(&lsblk_dev, fs_dump, &cmd_path) {
                // TODO: need partprobe ?
                if let Err(why) = mounts.mount_balena(true) {
                    error!(
                        "write_balena_os: failed mount balena partitions, error: {:?}",
                        why
                    );
                    sync();
                    return FlashResult::FailNonRecoverable;
                }

                if balena_write(mounts, fs_dump, base_path, &cmd_path) {
                    sync();
                    match call(
                        cmd_path[LSBLK_CMD].as_str(),
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
                    error!("write_balena_os: failed to initialise devices");
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
    fs_dump: &CheckedFSDump,
    base_path: &Path,
    cmd_path: &HashMap<&str, String>,
) -> bool {
    // TODO: try use device labels instead

    debug!(
        "Attempting to write file systems, base path: '{}'",
        base_path.display()
    );

    let tar_path = cmd_path[TAR_CMD].as_str();

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
        false
    }
}

fn sub_format(
    device: &Path,
    label: &str,
    is_fat: bool,
    check: &PartCheck,
    direct_io: bool,
    cmd_path: &HashMap<&str, String>,
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
        cmd_path[FAT_FMT_CMD].as_str()
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

        cmd_path[EXT_FMT_CMD].as_str()
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
    let cmd_res = match Command::new(&command)
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

fn format(
    lsblk_dev: &BlockDevice,
    fs_dump: &CheckedFSDump,
    cmd_path: &HashMap<&str, String>,
) -> bool {
    debug!("format: entered with: {:?}", lsblk_dev.name);
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

        debug!("format: wrote empty /et/mtab to make mkfs happy");

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

        debug!("format: check: {:?}, fat_check: {:?}", check, fat_check);
        let direct_io = if let Some(direct_io) = fs_dump.mkfs_direct {
            direct_io
        } else {
            false
        };

        let mut dev_idx: usize = 0;
        let mut part_idx: usize = 0;
        let has_ext_part = children.len() > PART_NAME.len();
        let dev_path = PathBuf::from("/dev");

        while (dev_idx < children.len())
            && (part_idx < PART_NAME.len())
            && (part_idx < PART_FSTYPE.len())
        {
            let index = children[dev_idx].index;

            if index != 4 || !has_ext_part {
                let part_label = PART_NAME[part_idx];
                let part_fs_type = PART_FSTYPE[part_idx];
                let part_path = path_append(&dev_path, &children[dev_idx].name);

                let is_fat = part_fs_type.contains("fat");
                let curr_check = if is_fat { fat_check } else { check };

                debug!(
                    "format: formatting partition {}/{} partidx: {}, path: {} with label: {} type: {}, is_fat: {} check: {:?} direct_io: {}",
                    dev_idx,
                    children.len(),
                    part_idx,
                    part_path.display(),
                    part_label,
                    part_fs_type,
                    is_fat,
                    curr_check,
                    direct_io
                );

                if !sub_format(
                    &part_path, part_label, is_fat, curr_check, direct_io, cmd_path,
                ) {
                    error!("format: failed to format partition {}/{} partidx: {}, path: {} with label: {} type: {}",
                    dev_idx,
                    children.len(),
                    part_idx,
                    part_path.display(),
                    part_label,
                    part_fs_type);
                    return false;
                }
                part_idx += 1;
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

fn partition_sfdisk(
    device: &Path,
    fs_dump: &CheckedFSDump,
    cmd_path: &HashMap<&str, String>,
) -> FlashResult {
    let dev_name = String::from(&*device.to_string_lossy());

    let part_stub = if Regex::new(r"^.*\d+$").unwrap().is_match(&dev_name) {
        let mut stub = String::from(&dev_name);
        stub.push('p');
        stub
    } else {
        dev_name.clone()
    };

    let mut part_string = String::new();

    // TODO: configure partition type

    debug!("partition_sfdisk: Writing label type: dos",);
    part_string.push_str("label : dos\n");

    /* TODO: generate random label-id or rather let sfdisk do the job ?
    part_string.push_str("label-id 0x{:x}\n", random_number: u32)
    */

    let alignment_blocks: u64 = DEFAULT_PARTITION_ALIGNMENT_KIB * 1024 / DEF_BLOCK_SIZE as u64;
    debug!(
        "partition_sfdisk: Alignment '{}'KiB, {} blocks",
        DEFAULT_PARTITION_ALIGNMENT_KIB, alignment_blocks
    );

    debug!(
        "partition_sfdisk: Writing resin-boot as 'size={},bootable,type=e' to '{}'",
        fs_dump.boot.blocks,
        device.display()
    );

    let mut start_block: u64 = alignment_blocks;
    let end_block: u64 = start_block + fs_dump.boot.blocks;

    let part_def = format!(
        "device : name={}1, start={} size={} bootable type=e \n",
        part_stub, start_block, fs_dump.boot.blocks
    );
    debug!(
        "partition_sfdisk: Writing resin-boot as '{}', end={}",
        part_def, end_block
    );
    part_string.push_str(&part_def);

    start_block = end_block;
    if (start_block % alignment_blocks) != 0 {
        start_block = (start_block / alignment_blocks + 1) * alignment_blocks;
    }

    let end_block: u64 = start_block + fs_dump.root_a.blocks;

    let part_def = format!(
        "device : name={}2, start={} size={} type=83 \n",
        part_stub, start_block, fs_dump.root_a.blocks
    );
    debug!(
        "partition_sfdisk: Writing resin-rootA as '{}', end={}",
        part_def, end_block
    );
    part_string.push_str(&part_def);

    start_block = end_block;
    if (start_block % alignment_blocks) != 0 {
        start_block = (start_block / alignment_blocks + 1) * alignment_blocks;
    }
    let end_block: u64 = start_block + fs_dump.root_b.blocks;

    let part_def = format!(
        "device : name={}3, start={} size={} type=83 \n",
        part_stub, start_block, fs_dump.root_b.blocks
    );
    debug!(
        "partition_sfdisk: Writing resin-rootB as '{}', end={}",
        part_def, end_block
    );
    part_string.push_str(&part_def);

    start_block = end_block;
    if (start_block % alignment_blocks) != 0 {
        start_block = (start_block / alignment_blocks + 1) * alignment_blocks;
    }

    let max_data = if let Some(max_data) = fs_dump.max_data {
        max_data
    } else {
        DEFAULT_MAX_DATA
    };

    let (part_def, end_block) = if max_data {
        (
            format!(
                "device : name={}4, start={} type=f \n",
                part_stub, start_block
            ),
            -1 as i64,
        )
    } else {
        (
            format!(
                "device : name={}4, start={} size={} type=f \n",
                part_stub, start_block, fs_dump.extended_blocks
            ),
            (start_block + fs_dump.extended_blocks) as i64,
        )
    };

    debug!(
        "partition_sfdisk: Writing extended partition as '{}', end={}",
        part_def, end_block
    );

    part_string.push_str(&part_def);

    start_block += alignment_blocks;
    let end_block: u64 = start_block + fs_dump.state.blocks;

    let part_def = format!(
        "device : name={}5, start={} size={} type=83 \n",
        part_stub, start_block, fs_dump.state.blocks
    );
    debug!(
        "partition_sfdisk: Writing resin-state as '{}', end={}",
        part_def, end_block
    );
    part_string.push_str(&part_def);

    // in dos extended partition at least 1 block offset is needed for the next extended entry
    // so align to next and add an extra alignment block
    start_block = end_block;
    if (start_block % alignment_blocks) != 0 {
        start_block = (start_block / alignment_blocks + 2) * alignment_blocks;
    } else {
        start_block += alignment_blocks;
    }

    let (part_def, end_block) = if max_data {
        (
            format!(
                "device : name={}6, start={} type=83 \n",
                part_stub, start_block,
            ),
            -1 as i64,
        )
    } else {
        (
            format!(
                "device : name={}6, start={} size={} type=83 \n",
                part_stub, start_block, fs_dump.data.blocks
            ),
            (start_block + fs_dump.data.blocks) as i64,
        )
    };

    debug!(
        "partition_sfdisk: Writing resin-data as '{}', end={}",
        part_def, end_block
    );
    part_string.push_str(&part_def);

    debug!("call_with_stdin: Writing: '{}'", part_string);

    match call_with_stdin(
        cmd_path[SFDISK_CMD].as_str(),
        &["-f", dev_name.as_str()],
        &mut part_string.as_bytes(),
        true,
    ) {
        Ok(cmd_res) => {
            if !cmd_res.status.success() {
                error!(
                    "sfdisk returned an error status: code: {:?}, stderr: {:?}",
                    cmd_res.status.code(),
                    cmd_res.stderr
                );
                FlashResult::FailNonRecoverable
            } else {
                sync();
                debug!("partition_sfdisk: sfdisk stdout: {:?}", cmd_res.stdout);
                FlashResult::Ok
            }
        }
        Err(why) => {
            error!(
                "Failed to run command : '{}' with args: {:?}, input: '{}' error: {:?}",
                cmd_path[SFDISK_CMD].as_str(),
                &[dev_name],
                part_string,
                why
            );
            FlashResult::FailRecoverable
        }
    }
}

fn part_reread(
    device: &Path,
    timeout: u64,
    num_partitions: usize,
    cmd_path: &HashMap<&str, String>,
) -> Result<BlockDevice, MigError> {
    debug!(
        "part_reread: entered with: '{}', timeout: {}, num_partitions: {}",
        device.display(),
        timeout,
        num_partitions
    );

    let start = SystemTime::now();
    thread::sleep(Duration::from_secs(1));

    if let Some(cmd) = cmd_path.get(PARTPROBE_CMD) {
        match call(&cmd, &[&device.to_string_lossy()], true) {
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
                debug!("part_reread: partprobe failed to execute '{}'", why);
            }
        }
    }

    loop {
        thread::sleep(Duration::from_secs(1));
        debug!(
            "part_reread: calling LsblkInfo::for_device('{}')",
            device.display()
        );
        let lsblk_dev = BlockDevice::from_device_path(device)?;
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

    /*
    fn partition_parted(device: &Path, fs_dump: &CheckedFSDump) -> FlashResult {
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
    */
}
