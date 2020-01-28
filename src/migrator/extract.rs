// the extractor module used by balena-extract- extract individual partitions from balena image to tar
// gzipped archives and print config snippet for balena-migrate.yml

use clap::{App, Arg};
use failure::ResultExt;
use log::{debug, error, info, trace, warn};
use mod_logger::{Level, LogDestination, Logger, NO_STREAM};
use nix::{
    mount::{mount, umount, MsFlags},
    unistd::sync,
};
use std::fs::{remove_dir, remove_file, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use serde_yaml;

use crate::{
    common::{
        call,
        config::balena_config::{FSDump, FileRef, ImageType, PartDump},
        file_digest::get_default_digest,
        path_append, MigErrCtx, MigError, MigErrorKind,
    },
    defs::FileType,
    linux::{
        disk_util::{Disk, PartitionIterator, PartitionReader, PartitionType},
        linux_common::{is_admin, is_file_type, mktemp, whereis},
        linux_defs::NIX_NONE,
        linux_defs::{FILE_CMD, LOSETUP_CMD, MKTEMP_CMD, TAR_CMD},
        stage2::{PART_FSTYPE, PART_NAME},
    },
};

const REQUIRED_CMDS: &[&str] = &[FILE_CMD, MKTEMP_CMD, TAR_CMD, LOSETUP_CMD];
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

// TODO: Extractor could modify config / save new ImageType with the usual downside of destroying comments & so on
// TODO: Save ImageType as yml file

pub fn extract() -> Result<(), MigError> {
    Logger::create();
    Logger::set_color(true);
    Logger::set_log_dest(&LogDestination::Stderr, NO_STREAM).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        "failed to set up logging",
    ))?;

    if !is_admin()? {
        error!("Please run this program as root");
        return Err(MigError::from(MigErrorKind::Displayed));
    }

    let mut extractor = Extractor::new()?;
    extractor.do_extract(None)?;
    Ok(())
}

impl Extractor {
    fn new() -> Result<Extractor, MigError> {
        trace!("new: entered");

        // define , collect command line options
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
                Arg::with_name("output-dir")
                    .short("o")
                    .long("output-dir")
                    .value_name("path")
                    .help("Sets the output directory"),
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

        match arg_matches.occurrences_of("verbose") {
            0 => (),
            1 => Logger::set_default_level(&Level::Info),
            2 => Logger::set_default_level(&Level::Debug),
            _ => Logger::set_default_level(&Level::Trace),
        }

        let out_path = if let Some(path) = arg_matches.value_of("output-dir") {
            path
        } else {
            "."
        };

        let work_dir = PathBuf::from(out_path)
            .canonicalize()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to cannonicalize path '{}'", out_path),
            ))?;

        info!("Using working directory '{}'", work_dir.display());

        let extract_device = if let Some(value) = arg_matches.value_of("device-type") {
            match value {
                // TODO: add more device types, why are there no RPI's
                "beaglebone-black" => String::from(value),
                "beaglebone-green" => String::from(value),
                _ => {
                    error!("Unsupported device type for extract: {}", value);
                    return Err(MigError::displayed());
                }
            }
        } else {
            error!("Missing the mandatory parameter extract-device",);
            return Err(MigError::displayed());
        };

        info!("Device type set to '{}'", extract_device);

        let image_file = if let Some(value) = arg_matches.value_of("image") {
            let file = PathBuf::from(value);
            if file.exists() {
                file.canonicalize().context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("Failed to canonicalize path '{}'", file.display()),
                ))?
            } else {
                error!("Could not find image file: '{}'", value);
                return Err(MigError::displayed());
            }
        } else {
            error!("Missing mandatory parameter image file.");
            return Err(MigError::displayed());
        };
        info!("Using image file '{}'", image_file.display());

        if !REQUIRED_CMDS.iter().all(|cmd| match whereis(cmd) {
            Ok(_cmd) => true,
            Err(why) => {
                error!(
                    "Could not find required command: '{}': error: {:?}",
                    cmd, why
                );
                false
            }
        }) {
            return Err(MigError::displayed());
        }

        if is_file_type(&image_file, &FileType::GZipOSImage)? {
            match Disk::from_gzip_img(&image_file) {
                Ok(gzip_img) => {
                    debug!("new: is gzipped image '{}'", image_file.display());
                    Ok(Extractor {
                        work_dir,
                        disk: gzip_img,
                        device_slug: extract_device,
                    })
                }
                Err(why) => {
                    error!(
                        "Unable to open the gzipped image file '{}', error: {:?}",
                        image_file.display(),
                        why
                    );
                    Err(MigError::displayed())
                }
            }
        } else if is_file_type(&image_file, &FileType::OSImage)? {
            match Disk::from_drive_file(&image_file, None) {
                Ok(plain_img) => {
                    debug!("new: is plain image '{}'", image_file.display());
                    Ok(Extractor {
                        work_dir,
                        disk: plain_img,
                        device_slug: extract_device,
                    })
                }
                Err(why) => {
                    error!(
                        "Unable to open the image file '{}', error: {:?}",
                        image_file.display(),
                        why
                    );
                    Err(MigError::displayed())
                }
            }
        } else {
            error!(
                "Unable to open the image file '{}', an unexpected file type was found",
                image_file.display(),
            );
            Err(MigError::displayed())
        }
    }

    pub fn do_extract(&mut self, output_path: Option<&Path>) -> Result<ImageType, MigError> {
        trace!("do_extract: entered");
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
        let mut partitions: Vec<Partition> = Vec::new();
        let mut part_iterator = PartitionIterator::new(&mut self.disk)?;
        let mut extended_blocks: u64 = 0;

        while let Some(raw_part) = part_iterator.next() {
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

            // TODO: partition names & types are hardcoded. Find a way to make this dynamic

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

            if extract_err.is_some() {
                break;
            }

            partitions.push(partition);
        }

        // TODO: try to umount, otherwise the following might fail
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
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!(
                        "No archive found for partition '{}', aborting",
                        partition.name
                    ),
                ));
            }
        }

        // TODO: expected number of partitions is hardcoded, make dynamic, same for above names, types
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
                &"Failed to serialize config to yaml".to_string(),
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

            let mut heap_buffer: Vec<u8> = vec![0; DEF_BUFFER_SIZE];
            let mut buffer = heap_buffer.as_mut_slice();
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
                            "Read write bytes mismatch writing to '{}'",
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

        // find/use the first unused loop device to loop mount partition content using losetup
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

        // get the name of the associated loop device ?
        let cmd_res = call(
            LOSETUP_CMD,
            &["-O", "name", "-j", &tmp_name.to_string_lossy()],
            true,
        )?;

        if !cmd_res.status.success() {
            return Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &"Failed to locate mounted loop device".to_string(),
            ));
        }

        let device = if let Some(output) = cmd_res.stdout.lines().last() {
            String::from(output)
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &"Failed to parse mounted loop device".to_string(),
            ));
        };

        debug!(
            "write_partition: mounting '{}' as '{}' on '{}'",
            tmp_name.display(),
            device,
            mountpoint.display()
        );

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
            path_append(work_dir, &format!("{}.tgz", partition.name))
        };

        #[cfg(feature = "extract_builtin_tar")]
        let write_tar = || -> Result<(), MigError> {
            // TODO: Try to archive using rust builtin tar / gzip,
            // Needs some more attention, adding files to fs root looks different from what external
            // tar does and archiving failed on rootA partition with
            //  { inner: Os { code: 2, kind: NotFound, message: "No such file or directory" } }

            info!("extract: using builtin tar");

            use flate2::{write::GzEncoder, Compression};
            use std::fs::File;
            use tar::Builder;

            let mut archive = Builder::new(GzEncoder::new(
                File::create(&arch_name).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed to create partition archive in file '{}'",
                        arch_name.display()
                    ),
                ))?,
                Compression::default(),
            ));

            archive
                .append_dir_all("./", mountpoint)
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed to add partition content to archive '{}'",
                        arch_name.display()
                    ),
                ))?;

            archive.finish().context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed create partition archive '{}'", arch_name.display()),
            ))?;

            Ok(())
        };

        #[cfg(not(feature = "extract_builtin_tar"))]
        let write_tar = || -> Result<(), MigError> {
            // TODO: replace with the above rust builtin tar when its ready

            info!("extract: using external tar");

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

            Ok(())
        };

        write_tar()?;

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
}
