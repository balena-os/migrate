use failure::{Fail, ResultExt};
use flate2::{write::GzEncoder, Compression};
use log::{debug, info, trace, warn};
use regex::Regex;
use std::fs::{create_dir, read_dir, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use tar::Builder;

use crate::common::{
    config::migrate_config::VolumeConfig, file_exists, path_append, MigErrCtx, MigError,
    MigErrorKind,
};

// Recurse through directories

fn archive_dir(
    dir_path: &Path,
    target_path: &Path,
    archive: &mut Builder<GzEncoder<File>>,
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
                    archive_dir(
                        &source_path,
                        &path_append(&target_path, &source_file),
                        archive,
                        &filter,
                    )?;
                } else {
                    if let Some(filter) = filter {
                        if filter.is_match(&source_path.to_string_lossy()) {
                            let target = path_append(target_path, &source_file);
                            archive
                                .append_path_with_name(&source_path, &target)
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
                        archive
                            .append_path_with_name(&source_path, &target)
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
            }
            Err(why) => {
                return Err(MigError::from(why.context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("Failed to read entry from "),
                ))));
            }
        }
    }

    Ok(written)
}

pub(crate) fn create(file: &Path, config: &[VolumeConfig]) -> Result<bool, MigError> {
    // TODO: stop selected services, containers, add this to backup config

    trace!("create entered with: '{}', {:?}", file.display(), config);

    let mut written = false;

    if config.len() > 0 {
        let mut archive = Builder::new(GzEncoder::new(
            File::create(file).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to create backup in file '{}'", file.display()),
            ))?,
            Compression::default(),
        ));

        debug!("creating new backup in '{}", file.display());

        for ref volume in config {
            info!("backup to volume: '{}'", volume.volume);

            for item in &volume.items {
                let item_src = PathBuf::from(&item.source);
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

                        if archive_dir(&item_src, &target_path, &mut archive, &filter)? {
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
                        archive.append_path_with_name(&item_src, &target).context(
                            MigErrCtx::from_remark(
                                MigErrorKind::Upstream,
                                &format!(
                                    "Failed to append '{}' to archive path '{}'",
                                    item_src.display(),
                                    target.display()
                                ),
                            ),
                        )?;
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

        archive.finish().context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to create backup archive: '{}'", file.display()),
        ))?;
        info!(
            "The backup was successfully written to '{}'",
            file.display()
        );
    } else {
        info!("The backup configuration was empty - nothing backed up");
    }

    Ok(written)
}
