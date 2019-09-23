use failure::ResultExt;
use log::{debug, error, info, trace, warn};
use nix::{
    mount::{mount, umount, MsFlags},
    unistd::sync,
};
use std::fs::{remove_dir, remove_file, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use mod_logger::{Logger, Level, LogDestination, NO_STREAM};
use clap::{App, Arg};

use serde_yaml;

use crate::{
    common::disk_util::PartitionType,
    common::{
        call,
        disk_util::{Disk, PartitionIterator, PartitionReader}, //  , ImageFile, GZipFile, PlainFile },
        file_digest::get_default_digest,
        path_append,
        MigErrCtx,
        MigError,
        MigErrorKind,
        config::balena_config::{ImageType,FileRef, PartDump, FSDump},
    },
    defs::FileType,
    defs::{PART_FSTYPE, PART_NAME},
    linux::{
        linux_common::{is_admin, is_file_type, mktemp, whereis},
        linux_defs::NIX_NONE,
        linux_defs::{FILE_CMD, LOSETUP_CMD, MKTEMP_CMD, TAR_CMD},
    },
};


// mod image_file;
// use image_file::ImageFile;

// mod gzip_file;
// use gzip_file::GZipFile;

// mod plain_file;
// use plain_file::PlainFile;

const REQUIRED_CMDS: &[&str] = &[
    FILE_CMD,
    MKTEMP_CMD,
    TAR_CMD,
    LOSETUP_CMD,
];
const DEF_BUFFER_SIZE: usize = 1024 * 1024;

const EXTRACT_FILE_TEMPLATE: &str = "extract.XXXXXXXXXX";
const MOUNTPOINT_TEMPLATE: &str = "mountpoint.XXXXXXXXXX";

pub(crate) struct Partition {
    pub name: &'static str,
    pub fstype: &'static str,
    pub ptype: u8,
    pub status: u8,
    pub start_lba: u64,
    pub num_sectors: u64,
    pub archive: Option<FileRef>,
}

pub(crate) struct Extractor {
    work_dir: PathBuf,
    device_slug: String,
    disk: Disk,
}

// TODO: Extractor could modify config / save new ImageType
// TODO: Save ImageType as yml file

pub fn extract() -> Result<(), MigError> {
    Logger::create();
    Logger::set_color(true);
    Logger::set_log_dest(&LogDestination::BufferStderr, NO_STREAM).context(
        MigErrCtx::from_remark(MigErrorKind::Upstream, "failed to set up logging"),
    )?;

    if !is_admin()? {
        error!("please run this program as root");
        return Err(MigError::from(MigErrorKind::Displayed));
    }

    let mut extractor = Extractor::new()?;
    extractor.do_extract(None)?;
    Ok(())
}

impl Extractor {
    fn new() -> Result<Extractor, MigError> {
        trace!("new: entered");

        let arg_matches = App::new("balena-extract")
            .version("0.1")
            .author("Thomas Runte <thomasr@balena.io>")
            .about("Extracts features from balena OS Images")
            .arg(
                Arg::with_name("image")
                    .required(true)
                    .help("use balena OS image"),
            )
            .arg(
                Arg::with_name("verbose")
                    .short("v")
                    .multiple(true)
                    .help("Sets the level of verbosity"),
            )
            .arg(
                Arg::with_name("device-type")
                    .short("d")
                    .long("device-type")
                    .value_name("type")
                    .required(true)
                    .help("specify image device slug for extraction"),
            )
            .get_matches();


        println!("Logger set, level  {}", arg_matches.occurrences_of("verbose"));

        match arg_matches.occurrences_of("verbose") {
            0 => (),
            1 => Logger::set_default_level(&Level::Info),
            2 => Logger::set_default_level(&Level::Debug),
            _ => Logger::set_default_level(&Level::Trace),
        }

        let work_dir = PathBuf::from(".").canonicalize().context(MigErrCtx::from_remark(MigErrorKind::Upstream, "Failed to cannonicalize path '.'", ))?;
        info!("Using working directory '{}'", work_dir.display());

        // TODO: support more devices
        let extract_device = if let Some(value) = arg_matches.value_of("device-type") {
            match value {
                "beaglebone-black" => String::from(value),
                "beaglebone-green" => String::from(value),
                _ => {
                    error!("Unsupported device type for extract: {}", value);
                    return Err(MigError::displayed());
                }
            }
        } else {
            error!("Missing the mandatory parameter extract-device", );
            return Err(MigError::displayed());
        };

        info!("Device type set to '{}'", extract_device);

        let image_file = if let Some(value) = arg_matches.value_of("image") {
            let file = PathBuf::from(value);
            if file.exists() {
                file.canonicalize().context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed to cannonicalize path '{}'", file.display())))?
            } else {
                error!("Could not find image file: '{}'", value);
                return Err(MigError::displayed())
            }
        } else {
            error!("No image file was specified.");
            return Err(MigError::displayed())
        };
        info!("Using image file '{}'", image_file.display());

        for command in REQUIRED_CMDS {
            match whereis(command) {
                Ok(_cmd_path) => (),
                Err(why) => {
                    error!(
                        "Could not find required command: '{}': error: {:?}",
                        command, why
                    );
                    return Err(MigError::displayed());
                }
            }
        }

        debug!("new: working with file '{}'", image_file.display());
        if is_file_type(&image_file, &FileType::GZipOSImage)? {
            match Disk::from_gzip_img(&image_file) {
                Ok(gzip_img) => {
                    debug!("new: is gzipped image '{}'", image_file.display());
                    return Ok(Extractor {
                        work_dir,
                        disk: gzip_img,
                        device_slug: extract_device,
                    });
                }
                Err(why) => {
                    error!(
                        "Unable to open the gzipped image file '{}', error: {:?}",
                        image_file.display(),
                        why
                    );
                    return Err(MigError::displayed());
                }
            }
        } else {
            if is_file_type(&image_file, &FileType::OSImage)? {
                match Disk::from_drive_file(&image_file, None) {
                    Ok(plain_img) => {
                        debug!("new: is plain image '{}'", image_file.display());
                        return Ok(Extractor {
                            work_dir,
                            disk: plain_img,
                            device_slug: extract_device,
                        });
                    }
                    Err(why) => {
                        error!(
                            "Unable to open the image file '{}', error: {:?}",
                            image_file.display(),
                            why
                        );
                        return Err(MigError::displayed());
                    }
                }
            } else {
                error!(
                    "Unable to open the image file '{}', an unexpected file type was found",
                    image_file.display(),
                );
                return Err(MigError::displayed());
            }
        }
    }

    pub fn do_extract(&mut self, output_path: Option<&Path>) -> Result<ImageType, MigError> {
        trace!("extract: entered");
        let work_dir = &self.work_dir;

        let mountpoint = match mktemp(true, Some(MOUNTPOINT_TEMPLATE), Some(work_dir)) {
            Ok(path) => path,
            Err(why) => {
                error!(
                    "Failed to create temporary mountpoint for image extraction, error: {:?}",
                    why
                );
                return Err(MigError::displayed());
            }
        };

        // make file name
        let tmp_name = match mktemp(false, Some(EXTRACT_FILE_TEMPLATE), Some(work_dir)) {
            Ok(path) => path,
            Err(why) => {
                error!(
                    "Failed to create temporary file for image extraction, error: {:?}",
                    why
                );
                return Err(MigError::displayed());
            }
        };

        let mut extract_err: Option<MigError> = None;
        // let mut part_extract_idx: usize = 0;

        let mut partitions: Vec<Partition> = Vec::new();

        let mut part_iterator = PartitionIterator::new(&mut self.disk)?;

        let mut extended_blocks: u64 = 0;

        loop {
            let raw_part = if let Some(raw_part) = part_iterator.next() {
                raw_part
            } else {
                break;
            };

            let part_idx = partitions.len();

            match PartitionType::from_ptype(raw_part.ptype) {
                PartitionType::Container => {
                    extended_blocks = raw_part.num_sectors;
                    continue;
                } // skip extended partition
                PartitionType::Fat | PartitionType::Linux => (), // expected partition
                _ => {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!("Encountered unexpected partition type {:x}", raw_part.ptype),
                    ));
                }
            }

            let mut partition = Partition {
                name: PART_NAME[part_idx],
                fstype: PART_FSTYPE[part_idx],
                status: raw_part.status,
                ptype: raw_part.ptype,
                start_lba: raw_part.start_lba,
                num_sectors: raw_part.num_sectors,
                archive: None,
            };

            let mut part_reader =
                PartitionReader::from_part_iterator(&raw_part, &mut part_iterator);

            match Extractor::write_partition(
                self.work_dir.as_path(),
                &mut part_reader,
                &mut partition,
                &tmp_name,
                &mountpoint,
                output_path,
            ) {
                Ok(_) => {
                    info!(
                        "extracted partition: {}: to '{}'",
                        partition.name,
                        partition.archive.as_ref().unwrap().path.display()
                    );
                }
                Err(why) => {
                    error!(
                        "Failed to write partition {}: error: {:?}",
                        partition.name, why
                    );
                    extract_err = Some(why);
                    break;
                }
            }

            if let Some(_) = extract_err {
                break;
            }

            partitions.push(partition);
        }

        // TODO: try to umount
        let _res = remove_dir(&mountpoint);
        let _res = remove_file(&tmp_name);

        // late error exit after cleanup
        if let Some(why) = extract_err {
            return Err(why);
        }

        for partition in &mut partitions {
            if let Some(ref mut file_ref) = partition.archive {
                file_ref.path = file_ref
                    .path
                    .strip_prefix(work_dir)
                    .context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "Failed to strip workdir '{}' off path '{}'",
                            work_dir.display(),
                            file_ref.path.display()
                        ),
                    ))?
                    .to_path_buf();
            }
        }

        if partitions.len() == 5 {
            let res = ImageType::FileSystems(FSDump {
                device_slug: self.device_slug.clone(),
                check: None,
                max_data: None,
                mkfs_direct: None,
                extended_blocks,
                boot: PartDump {
                    archive: partitions[0].archive.as_ref().unwrap().clone(),
                    blocks: partitions[0].num_sectors,
                },
                root_a: PartDump {
                    archive: partitions[1].archive.as_ref().unwrap().clone(),
                    blocks: partitions[1].num_sectors,
                },
                root_b: PartDump {
                    archive: partitions[2].archive.as_ref().unwrap().clone(),
                    blocks: partitions[2].num_sectors,
                },
                state: PartDump {
                    archive: partitions[3].archive.as_ref().unwrap().clone(),
                    blocks: partitions[3].num_sectors,
                },
                data: PartDump {
                    archive: partitions[4].archive.as_ref().unwrap().clone(),
                    blocks: partitions[4].num_sectors,
                },
            });

            debug!("res: {:?}", &res);

            let yaml_config = serde_yaml::to_string(&res).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to serialize config to yaml"),
            ))?;

            let mut entabbed_cfg = String::new();
            let lines = yaml_config.lines();
            for line in lines {
                entabbed_cfg.push_str(&format!("    {}\n", line));
            }

            println!("image config:");
            println!("{}", entabbed_cfg);

            Ok(res)
        } else {
            error!(
                "Unexpected number of partitions found in image: '{}', {}",
                self.disk.get_image_file().display(),
                partitions.len()
            );
            Err(MigError::displayed())
        }
    }

    fn write_partition(
        work_dir: &Path,
        part_reader: &mut PartitionReader,
        partition: &mut Partition,
        tmp_name: &Path,
        mountpoint: &Path,
        output_path: Option<&Path>,
    ) -> Result<(), MigError> {
        trace!(
            "write_partition: entered with tmp_name: '{}', mountpoint: '{}'",
            tmp_name.display(),
            mountpoint.display()
        );

        // TODO: cleanup on failure

        {
            // read partition contents to file
            let mut tmp_file = OpenOptions::new()
                .create(false)
                .write(true)
                .truncate(true)
                .open(&tmp_name)
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("Failed to opent temp file '{}'", tmp_name.display()),
                ))?;

            // TODO: check free disk space

            let mut buffer: [u8; DEF_BUFFER_SIZE] = [0; DEF_BUFFER_SIZE];
            loop {
                let bytes_read = part_reader
                    .read(&mut buffer)
                    .context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        "Failed to read",
                    ))?;

                if bytes_read == 0 {
                    break;
                }

                let bytes_written =
                    tmp_file
                        .write(&buffer[0..bytes_read])
                        .context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!("Failed to write to '{}'", tmp_name.display()),
                        ))?;

                if bytes_read != bytes_written {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!(
                            "Read write bytes mismatch witing to '{}'",
                            tmp_name.display()
                        ),
                    ));
                }
            }

            debug!(
                "write_partition: partition written to '{}'",
                tmp_name.display()
            );
        }

        let cmd_res = call(LOSETUP_CMD, &["-f", &tmp_name.to_string_lossy()], true)?;

        if !cmd_res.status.success() {
            return Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &format!(
                    "Failed to loop mount extracted partition: {}",
                    cmd_res.stderr
                ),
            ));
        }

        let cmd_res = call(
            LOSETUP_CMD,
            &["-O", "name", "-j", &tmp_name.to_string_lossy()],
            true,
        )?;

        if !cmd_res.status.success() {
            return Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &format!("Failed to locate mounted loop device"),
            ));
        }

        let device = if let Some(output) = cmd_res.stdout.lines().into_iter().last() {
            String::from(output)
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &format!("Failed to parse mounted loop device"),
            ));
        };

        debug!(
            "write_partition: mounting '{}' as '{}' on '{}'",
            tmp_name.display(),
            device,
            mountpoint.display()
        );

        // TODO: use losetup and then mount, mount -o loop seems to not work in ubuntu-14

        mount(
            Some(device.as_str()),
            &mountpoint.to_path_buf(),
            Some(partition.fstype.as_bytes()),
            MsFlags::empty(),
            NIX_NONE,
        )
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Failed to mount loop device '{}' to '{}' with fstype: {:?}",
                device,
                &mountpoint.display(),
                partition.fstype
            ),
        ))?;

        let arch_name = if let Some(output_path) = output_path {
            path_append(output_path, &format!("{}.tgz", partition.name))
        } else {
            path_append(
                work_dir,
                &format!("{}.tgz", partition.name),
            )
        };

        // TODO: Try to archive using rust builtin tar / gzip have to traverse directories myself

        let cmd_res = call(
            TAR_CMD,
            &[
                "-czf",
                &arch_name.to_string_lossy(),
                "-C",
                &mountpoint.to_string_lossy(),
                ".",
            ],
            true,
        )?;

        if !cmd_res.status.success() {
            return Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &format!(
                    "Failed to archive extracted partition, msg: {}",
                    cmd_res.stderr
                ),
            ));
        }

        sync();
        thread::sleep(Duration::from_secs(1));

        debug!("write_partition: unmounting '{}'", mountpoint.display());
        umount(mountpoint).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("failed to unmount '{}'", mountpoint.display()),
        ))?;

        let cmd_res = call(LOSETUP_CMD, &["-d", &device], true)?;

        if !cmd_res.status.success() {
            return Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &format!("Failed to remove loop ,mount,  {}", cmd_res.stderr),
            ));
        }

        debug!(
            "write_partition: extracted partition '{}' to '{}'",
            partition.name,
            arch_name.display()
        );

        let digest = match get_default_digest(&arch_name) {
            Ok(digest) => Some(digest),
            Err(why) => {
                warn!(
                    "Failed to create digest for file: '{}', error: {:?}",
                    arch_name.display(),
                    why
                );
                None
            }
        };

        partition.archive = Some(FileRef {
            path: arch_name.canonicalize().context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to canonicalize path: '{}'", arch_name.display()),
            ))?,
            hash: digest,
        });

        Ok(())
    }

    /*
        // Read partition table at offset up to the first empty or extended partition
        // return offset of next partition table for extended partition or None for end of table

        // TODO: ensure that about using 0 size partition as

        fn read_part_tbl(
            &mut self,
            offset: u64,
            table: &mut Vec<Partition>,
        ) -> Result<Option<u64>, MigError> {
            trace!("read_part_tbl: entered with offset {}", offset);
            let mut buffer: [u8; DEF_BLOCK_SIZE] = [0; DEF_BLOCK_SIZE];

            self.image_file
                .fill(offset * DEF_BLOCK_SIZE as u64, &mut buffer)?;

            let mbr: MasterBootRecord = unsafe { mem::transmute(buffer) };

            if (mbr.boot_sig1 != 0x55) || (mbr.boot_sig2 != 0xAA) {
                error!(
                    "invalid mbr sig1: {:x}, sig2: {:x}",
                    mbr.boot_sig1, mbr.boot_sig2
                );
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    "unexpeted signatures found in partition table",
                ));
            }

            for partition in &mbr.part_tbl {
                let part_idx = table.len();

                if part_idx >= PART_NAME.len() || partition.num_sectors == 0 {
                    return Ok(None);
                }

                if (partition.ptype == 0xF) || (partition.ptype == 0x5) {
                    debug!(
                        "return extended partition offset: {}",
                        offset + partition.first_lba as u64
                    );
                    return Ok(Some(offset + partition.first_lba as u64));
                } else {
                    let part_info = Partition {
                        name: PART_NAME[part_idx],
                        fstype: PART_FSTYPE[part_idx],
                        start_lba: offset + partition.first_lba as u64,
                        num_sectors: partition.num_sectors as u64,
                        ptype: partition.ptype,
                        status: partition.status,
                        archive: None,
                    };

                    debug!(
                        "partition name: {}, fstype: {}, status: {:x}, type: {:x}, start: {}, size: {}",
                        part_info.name,
                        part_info.fstype,
                        part_info.status,
                        part_info.ptype,
                        part_info.start_lba,
                        part_info.num_sectors
                    );

                    table.push(part_info);
                }
            }
            debug!("return no further offset");
            Ok(None)
        }

    */
}
