use std::path::{Path, PathBuf};
use log::{debug, error, info};

use crate::{
    common::{
        MigError,
        config::{Config,balena_config::ImageType,},
        disk_util::{Disk, PartitionIterator, },
        file_info::{FileInfo, FileType},

    },
    linux::{
        linux_common::mktemp,
        ensured_cmds::{EnsuredCmds, FILE_CMD, LOSETUP_CMD, LSBLK_CMD, MKTEMP_CMD, },
    },
};
use crate::common::path_append;

const REQUIRED_CMDS: &[&str] = &[
    FILE_CMD,   // for FileInfo
    LSBLK_CMD,  // for LsblkInfo
    MKTEMP_CMD, // for linux_cmmon::mktemp
    LOSETUP_CMD, // locally
];

// const EXTRACT_FILE_TEMPLATE: &str = "extract.XXXXXXXXXX";
const MOUNTPOINT_TEMPLATE: &str = "mountpoint.XXXXXXXXXX";

pub(crate) struct FlashExtractor {
    cmds: EnsuredCmds,
    config: Config,
    // device_slug: String,
    disk: Disk,
    target_file: PathBuf,
}

impl FlashExtractor {
    pub fn new(config: Config) -> Result<FlashExtractor, MigError> {
        let mut cmds = EnsuredCmds::new();
        if let Err(why) = cmds.ensure_cmds(REQUIRED_CMDS) {
            error!(
                "Some Required commands could not be found, error: {:?}",
                why
            );
            return Err(MigError::displayed());
        }

        let image_file = if let ImageType::Flasher(image_file) = config.balena.get_image_path() {
            image_file
        } else {
            error!("The image does not point to a flash image",);
            return Err(MigError::displayed());
        };

        let image_info = FileInfo::new(image_file, config.migrate.get_work_dir())?;

        // TODO: make extract file name - derive from original filename OS type ?
        // TODO: check if file exists (force option for overwrite ?) ?
        let target_file = path_append(config.migrate.get_work_dir(), balena-image.gz);

        if let Some(image_info) = image_info {
            debug!("new: working with file '{}'", image_info.path.display());
            if image_info.is_type(&cmds, &FileType::GZipOSImage)? {
                match Disk::from_gzip_img(&image_info.path) {
                    Ok(gzip_img) => {
                        debug!("new: is gzipped image '{}'", image_info.path.display());
                        return Ok(FlashExtractor {
                            cmds,
                            config,
                            disk: gzip_img,
                            target_file,
                            // device_slug: String::from(extract_device),
                        });
                    }
                    Err(why) => {
                        error!(
                            "Unable to open the gzipped image file '{}', error: {:?}",
                            image_info.path.display(),
                            why
                        );
                        return Err(MigError::displayed());
                    }
                }
            } else {
                if image_info.is_type(&cmds, &FileType::OSImage)? {
                    match Disk::from_drive_file(&image_info.path, None) {
                        Ok(plain_img) => {
                            debug!("new: is plain image '{}'", image_info.path.display());
                            return Ok(FlashExtractor {
                                cmds,
                                config,
                                disk: plain_img,
                                target_file,
                            });
                        }
                        Err(why) => {
                            error!(
                                "Unable to open the image file '{}', error: {:?}",
                                image_info.path.display(),
                                why
                            );
                            return Err(MigError::displayed());
                        }
                    }
                } else {
                    error!(
                        "Found an unexpected file type in '{}', not an OS image",
                        image_info.path.display()
                    );
                    return Err(MigError::displayed());
                }
            }
        } else {
            error!(
                "The image file could not be found: '{}'",
                image_file.display()
            );
            Err(MigError::displayed())
        }
    }

    pub fn extract(&mut self, output_path: Option<&Path>) -> Result<ImageType, MigError> {
        trace!("extract: entered");

        let mountpoint = match mktemp(
            &self.cmds,
            true,
            Some(MOUNTPOINT_TEMPLATE),
            None, // Some(self.config.migrate.get_work_dir()),
        ) {
            Ok(path) => path,
            Err(why) => {
                error!(
                    "Failed to create temporary mountpoint for image extraction, error: {:?}",
                    why
                );
                return Err(MigError::displayed());
            }
        };

        // let mut partitions: Vec<Partition> = Vec::new();
        let mut part_iterator = PartitionIterator::new(&mut self.disk)?;
        loop {

        }




        unimplemented!();
    }
}
