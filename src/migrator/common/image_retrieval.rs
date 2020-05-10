use std::fs::{self, create_dir, remove_dir, File, OpenOptions};
use std::io::copy;
use std::path::PathBuf;

use log::{debug, error, info, warn, Level};

use semver::{Identifier, Version, VersionReq};

use crate::common::call;
use crate::linux::linux_defs::{LOSETUP_CMD, NIX_NONE};
use crate::{
    common::{
        api_calls::{get_os_image, get_os_versions},
        migrate_info::MigrateInfo,
        path_append, MigErrCtx, MigError, MigErrorKind,
    },
    linux::disk_util::{Disk, PartitionIterator, PartitionReader},
};

use crate::common::api_calls::Versions;
use crate::common::stream_progress::StreamProgress;
use failure::ResultExt;
use flate2::{Compression, GzBuilder};
use nix::mount::{mount, umount, MsFlags};

const FLASHER_DEVICES: [&str; 1] = ["intel-nuc"];
const SUPPORTED_DEVICES: [&str; 2] = ["raspberrypi3", "intel-nuc"];

fn parse_versions(versions: &Versions) -> Vec<Version> {
    let mut sem_vers: Vec<Version> = versions
        .versions
        .iter()
        .map(|ver_str| Version::parse(ver_str))
        .filter_map(|ver_res| match ver_res {
            Ok(version) => Some(version),
            Err(why) => {
                error!("Failed to parse version, error: {:?}", why);
                None
            }
        })
        .collect();
    sem_vers.sort();
    sem_vers.reverse();
    sem_vers
}

#[allow(clippy::cognitive_complexity)]
pub(crate) fn download_image(
    mig_info: &mut MigrateInfo,
    device_type: &str,
    version: &str,
) -> Result<PathBuf, MigError> {
    if !SUPPORTED_DEVICES.contains(&device_type) {
        error!(
            "OS download is not supported for device type {}",
            device_type
        );
        return Err(MigError::displayed());
    }

    if let Some(api_key) = mig_info.get_api_key() {
        let api_endpoint = mig_info.get_api_endpoint();

        let versions = get_os_versions(&api_endpoint, &api_key, device_type)?;

        let version = match version {
            "latest" => {
                info!("Selected latest version ({}) for download", versions.latest);
                Version::parse(&versions.latest).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("Failed to parse version from '{}'", versions.latest),
                ))?
            }
            "default" => {
                let mut found: Option<Version> = None;
                for cmp_ver in parse_versions(&versions) {
                    debug!("Looking at version {}", cmp_ver);
                    if cmp_ver.is_prerelease() {
                        continue;
                    } else if cmp_ver
                        .build
                        .contains(&Identifier::AlphaNumeric("prod".to_string()))
                    {
                        found = Some(cmp_ver);
                        break;
                    }
                }

                if let Some(found) = found {
                    info!("Selected default version ({}) for download", found);
                    found
                } else {
                    error!("No version found for '{}'", version);
                    return Err(MigError::displayed());
                }
            }
            _ => {
                if version.starts_with('^') || version.starts_with('~') {
                    let ver_req = VersionReq::parse(version).context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("Failed to parse version from '{}'", version),
                    ))?;
                    let mut found: Option<Version> = None;
                    for cmp_ver in parse_versions(&versions) {
                        if ver_req.matches(&cmp_ver)
                            && !cmp_ver.is_prerelease()
                            && cmp_ver
                                .build
                                .contains(&Identifier::AlphaNumeric("prod".to_string()))
                        {
                            found = Some(cmp_ver);
                            break;
                        }
                    }
                    if let Some(found) = found {
                        info!("Selected version {} for download", found);
                        found
                    } else {
                        error!("No version found for '{}'", version);
                        return Err(MigError::displayed());
                    }
                } else {
                    let ver_req = Version::parse(version).context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("Failed to parse version from '{}'", version),
                    ))?;

                    let mut found: Option<Version> = None;
                    for cmp_ver in parse_versions(&versions) {
                        if ver_req == cmp_ver
                            && !cmp_ver.is_prerelease()
                            && (cmp_ver.build == ver_req.build
                                || cmp_ver
                                    .build
                                    .contains(&Identifier::AlphaNumeric("prod".to_string())))
                        {
                            found = Some(cmp_ver);
                            break;
                        }
                    }
                    if let Some(found) = found {
                        info!("Selected version {} for download", found);
                        found
                    } else {
                        error!("No version found for '{}'", version);
                        return Err(MigError::displayed());
                    }
                }
            }
        };

        info!(
            "Downloading Balena OS image, selected version is: '{}'",
            version.to_string()
        );

        // TODO: extract OS image for flasher

        let stream = get_os_image(&api_endpoint, &api_key, device_type, &version.to_string())?;

        let img_file_name = path_append(
            &mig_info.work_path.path,
            &format!(
                "balena-cloud-{}-{}.img.gz",
                device_type,
                version.to_string()
            ),
        );

        if FLASHER_DEVICES.contains(&device_type) {
            let progress = StreamProgress::new(stream, 10, Level::Info, None);
            let mut disk = Disk::from_gzip_stream(progress)?;
            let mut part_iterator = PartitionIterator::new(&mut disk)?;
            if let Some(part_info) = part_iterator.nth(1) {
                let mut reader =
                    PartitionReader::from_part_iterator(&part_info, &mut part_iterator);
                let extract_file_name = path_append(&mig_info.work_path.path, "root_a.img");
                let mut tmp_file =
                    File::create(&extract_file_name).context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "Failed to create temporary file '{}'",
                            extract_file_name.display()
                        ),
                    ))?;

                // TODO: show progress
                copy(&mut reader, &mut tmp_file).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed to extract root_a partition to temporary file '{}'",
                        extract_file_name.display()
                    ),
                ))?;

                info!(
                    "Finished root_a partition extraction, now mounting to extract balena OS image"
                );

                let cmd_res = call(
                    LOSETUP_CMD,
                    &["--show", "-f", &*extract_file_name.to_string_lossy()],
                    true,
                )?;
                let loop_dev = if !cmd_res.status.success() {
                    error!(
                        "Failed to loop mount root_a partition from file '{}'",
                        extract_file_name.display()
                    );
                    return Err(MigError::displayed());
                } else {
                    cmd_res.stdout
                };
                debug!("loop device is '{}'", loop_dev);

                let mount_path = path_append(&mig_info.work_path.path, "mnt_root_a");
                if !mount_path.exists() {
                    create_dir(&mount_path).context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("Failed to create directory '{}'", mount_path.display()),
                    ))?;
                }

                debug!("mount path is '{}'", mount_path.display());
                #[allow(clippy::string_lit_as_bytes)]
                mount(
                    Some(loop_dev.as_bytes()),
                    &mount_path,
                    Some("ext4".as_bytes()),
                    MsFlags::empty(),
                    NIX_NONE,
                )
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed to mount '{}' on '{}",
                        loop_dev,
                        mount_path.display()
                    ),
                ))?;

                let img_path = match device_type {
                    "intel-nuc" => {
                        path_append(&mount_path, "opt/resin-image-genericx86-64.resinos-img")
                    }
                    _ => {
                        error!(
                            "Encountered undefined image name for device type {}",
                            device_type
                        );
                        return Err(MigError::displayed());
                    }
                };

                debug!("image path is '{}'", img_path.display());

                {
                    let mut gz_writer = GzBuilder::new().write(
                        File::create(&img_file_name).context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!(
                                "Failed to open image file for writing: '{}'",
                                img_file_name.display()
                            ),
                        ))?,
                        Compression::best(),
                    );

                    let img_reader = OpenOptions::new().read(true).open(&img_path).context(
                        MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!(
                                "Failed to open image file for reading: '{}'",
                                img_path.display()
                            ),
                        ),
                    )?;

                    info!("Recompressing OS image to {}", img_file_name.display());

                    let size = if let Ok(metadata) = img_reader.metadata() {
                        Some(metadata.len())
                    } else {
                        None
                    };

                    let mut stream_progress =
                        StreamProgress::new(img_reader, 10, Level::Info, size);

                    copy(&mut stream_progress, &mut gz_writer).context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!(
                            "Failed to compress image '{}' to '{}'",
                            img_path.display(),
                            img_file_name.display()
                        ),
                    ))?;
                }

                info!(
                    "The balena OS image was successfully written to '{}', cleaning up",
                    img_file_name.display()
                );

                match umount(&mount_path) {
                    Ok(_) => {
                        if let Err(why) = remove_dir(&mount_path) {
                            warn!(
                                "Failed to remove mount temporary directory '{}', error: {:?}",
                                mount_path.display(),
                                why
                            );
                        }
                    }
                    Err(why) => {
                        warn!(
                            "Failed to unmount temporary mount from '{}', error: {:?}",
                            mount_path.display(),
                            why
                        );
                    }
                }

                match call(LOSETUP_CMD, &["-d", &loop_dev], true) {
                    Ok(cmd_res) => {
                        if !cmd_res.status.success() {
                            warn!(
                                "Failed to remove loop device '{}', stderr: '{}'",
                                loop_dev, cmd_res.stderr
                            );
                        }
                    }
                    Err(why) => {
                        warn!(
                            "Failed to remove loop device '{}', error: {:?}",
                            loop_dev, why
                        );
                    }
                };

                if let Err(why) = fs::remove_file(&extract_file_name) {
                    warn!(
                        "Failed to remove extracted partition '{}', error: {:?}",
                        extract_file_name.display(),
                        why
                    );
                }
            }
        } else {
            debug!("Downloading file '{}'", img_file_name.display());
            let mut file = File::create(&img_file_name).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to create file: '{}'", img_file_name.display()),
            ))?;

            // TODO: show progress
            let mut progress = StreamProgress::new(stream, 10, Level::Info, None);
            copy(&mut progress, &mut file).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to write downloaded data to '{}'",
                    img_file_name.display()
                ),
            ))?;
            info!(
                "The balena OS image was successfully written to '{}'",
                img_file_name.display()
            );
        }

        Ok(img_file_name)
    } else {
        error!("No api-key found in config.json - unable to retrieve os-image");
        Err(MigError::displayed())
    }
}
