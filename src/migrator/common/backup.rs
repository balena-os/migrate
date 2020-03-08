use failure::{Fail, ResultExt};
use log::{debug, info, trace};
use regex::Regex;
use std::fs::read_dir;
use std::path::{Path, PathBuf};

use crate::common::{
    config::migrate_config::VolumeConfig, path_append, MigErrCtx, MigError, MigErrorKind,
};

use crate::common::os_api::{OSApi, OSApiImpl};

// Recurse through directories
mod archiver;
use crate::common::backup::archiver::Archiver;

mod rust_tar_archiver;
use crate::common::backup::rust_tar_archiver::RustTarArchiver;

// mod ext_tar_archiver;
// use crate::common::backup::ext_tar_archiver::ExtTarArchiver;

fn archive_dir<'a>(
    dir_path: &Path,
    target_path: &Path,
    archiver: &'a mut impl Archiver,
    filter: &Option<Regex>,
) -> Result<bool, MigError> {
    trace!(
        "archive_dir: dir_path: '{}', target_path: '{}' filter: {:?}",
        dir_path.display(),
        target_path.display(),
        filter
    );
    let mut written = false;

    for entry in read_dir(dir_path).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!(
            "Failed to list directory backup source: '{}'",
            dir_path.display()
        ),
    ))? {
        match entry {
            Ok(dir_entry) => {
                let source_path = dir_entry.path();
                let source_file = source_path.file_name().unwrap();
                debug!("processing source: '{}'", source_path.display());
                let metadata = dir_entry.metadata().context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed to retrieve metadata for file: '{}'",
                        source_path.display()
                    ),
                ))?;

                if metadata.is_dir() {
                    if archive_dir(
                        &source_path,
                        &path_append(&target_path, &source_file),
                        archiver,
                        &filter,
                    )? {
                        written = true;
                    }
                } else if let Some(filter) = filter {
                    if filter.is_match(&source_path.to_string_lossy()) {
                        let target = path_append(target_path, &source_file);
                        archiver
                            .add_file(target.as_path(), source_path.as_path())
                            .context(MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!(
                                    "Failed to append file: '{}' to archive path: '{}'",
                                    source_path.display(),
                                    target.display()
                                ),
                            ))?;
                        written = true;
                        debug!(
                            "appended source: '{}'  to archive as '{}'",
                            source_path.display(),
                            target.display()
                        );
                    } else {
                        debug!("No match on file: '{}'", &source_path.display());
                    }
                } else {
                    let target = path_append(target_path, &source_file);
                    archiver
                        .add_file(target.as_path(), source_path.as_path())
                        .context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!(
                                "Failed to append file: '{}' to archive path: '{}'",
                                source_path.display(),
                                target.display()
                            ),
                        ))?;
                    written = true;
                    debug!(
                        "appended source: '{}'  to archive as '{}'",
                        source_path.display(),
                        target.display()
                    );
                }
            }
            Err(why) => {
                return Err(MigError::from(why.context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &"Failed to read entry from ".to_string(),
                ))));
            }
        }
    }

    Ok(written)
}

/*
#[cfg(target_os = "linux")]
pub(crate) fn create_ext(file: &Path, config: &[VolumeConfig]) -> Result<bool, MigError> {
    if !config.is_empty() {
        debug!("creating new backup in '{}", file.display());
        let mut archiver = ExtTarArchiver::new(file)?;
        create_int(&mut archiver, config)
    } else {
        info!("The backup configuration was empty - nothing backed up");
        Ok(false)
    }
} */

pub(crate) fn create(file: &Path, config: &[VolumeConfig]) -> Result<bool, MigError> {
    if !config.is_empty() {
        debug!("creating new backup in '{}", file.display());
        let mut archiver = RustTarArchiver::new(file)?;
        create_int(&mut archiver, config)
    } else {
        info!("The backup configuration was empty - nothing backed up");
        Ok(false)
    }
}

fn create_int<'a>(
    archiver: &'a mut impl Archiver,
    config: &[VolumeConfig],
) -> Result<bool, MigError> {
    // TODO: stop selected services, containers, add this to backup config

    trace!("create_int entered with: {:?}", config);

    let mut written = false;
    let os_api = OSApiImpl::new()?;
    for volume in config.iter() {
        info!("backup to volume: '{}'", volume.volume);

        for item in &volume.items {
            let item_src =
                os_api
                    .canonicalize(Path::new(&item.source))
                    .context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("Failed to process source '{}'", item.source),
                    ))?;

            debug!("processing item: source. '{}'", item_src.display());

            if let Ok(metadata) = item_src.metadata() {
                if metadata.is_dir() {
                    let target_path = if let Some(ref target) = item.target {
                        path_append(PathBuf::from(&volume.volume), target)
                    } else {
                        PathBuf::from(&volume.volume)
                    };

                    debug!("source: '{}' is a directory", item_src.display());
                    let filter = if let Some(ref filter) = item.filter {
                        Some(Regex::new(filter).context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!(
                                "Failed to create regular expression from filter '{}'",
                                filter
                            ),
                        ))?)
                    } else {
                        None
                    };

                    if archive_dir(&item_src, &target_path, archiver, &filter)? {
                        written = true;
                    }
                } else {
                    debug!("source: '{}' is a file", item_src.display());
                    let target = if let Some(ref target) = item.target {
                        path_append(PathBuf::from(&volume.volume), target)
                    } else {
                        path_append(
                            PathBuf::from(&volume.volume),
                            &item_src.file_name().unwrap(),
                        )
                    };

                    debug!("target: '{}'", target.display());
                    archiver
                        .add_file(target.as_path(), item_src.as_path())
                        .context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!(
                                "Failed to append '{}' to archive path '{}'",
                                item_src.display(),
                                target.display()
                            ),
                        ))?;
                    written = true;
                    debug!(
                        "appended source: '{}'  to archive as '{}'",
                        item_src.display(),
                        target.display()
                    );
                }
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::NotFound,
                    &format!("Missing source for backup: '{}'", item.source),
                ));
            }
        }
    }

    archiver.finish().context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        "Failed to create backup archive",
    ))?;

    debug!("create_int: returning {}", written);
    Ok(written)
}
