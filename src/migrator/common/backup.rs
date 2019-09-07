use failure::{Fail, ResultExt};
use flate2::{write::GzEncoder, Compression};
use log::{debug, error, info, trace, warn};
use regex::Regex;
use std::fs::{create_dir_all, read_dir, remove_dir_all, File};
use std::path::{Path, PathBuf};
use tar::Builder;

#[cfg(target_os = "linux")]
use std::os::unix::fs::symlink;

use crate::common::{
    config::migrate_config::VolumeConfig, dir_exists, path_append, MigErrCtx, MigError,
    MigErrorKind,
};
use crate::defs::BACKUP_FILE;
use crate::linux::ensured_cmds::TAR_CMD;
use crate::linux::{EnsuredCmds, MKTEMP_CMD};

// Recurse through directories

trait Archiver {
    fn add_file(&mut self, target: &Path, source: &Path) -> Result<(), MigError>;
    fn finish(&mut self) -> Result<(), MigError>;
}

pub struct RustTarArchiver {
    archive: Builder<GzEncoder<File>>,
}

impl RustTarArchiver {
    fn new<P: AsRef<Path>>(file: P) -> Result<RustTarArchiver, MigError> {
        Ok(RustTarArchiver {
            archive: Builder::new(GzEncoder::new(
                File::create(file.as_ref()).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed to create backup in file '{}'",
                        file.as_ref().display()
                    ),
                ))?,
                Compression::default(),
            )),
        })
    }
}

impl Archiver for RustTarArchiver {
    fn add_file(&mut self, target: &Path, source: &Path) -> Result<(), MigError> {
        Ok(self
            .archive
            .append_path_with_name(&source, &target)
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to append file: '{}' to archive path: '{}'",
                    source.display(),
                    target.display()
                ),
            ))?)
    }

    fn finish(&mut self) -> Result<(), MigError> {
        Ok(self.archive.finish().context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            "Failed to create backup archive",
        ))?)
    }
}

#[cfg(target_os = "linux")]
pub struct ExtTarArchiver<'a> {
    cmds: &'a EnsuredCmds,
    tmp_dir: PathBuf,
    archive: PathBuf,
}

#[cfg(target_os = "linux")]
impl ExtTarArchiver<'_> {
    fn new<P: AsRef<Path>>(cmds: &EnsuredCmds, file: P) -> Result<ExtTarArchiver, MigError> {
        let cmd_res = cmds
            .call(MKTEMP_CMD, &["-d"], true)
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                "failed to create temporary directory for backup",
            ))?;

        if !cmd_res.status.success() {
            error!("Failed to create temporary directory");
            return Err(MigError::displayed());
        }

        Ok(ExtTarArchiver {
            cmds,
            tmp_dir: PathBuf::from(cmd_res.stdout),
            archive: PathBuf::from(file.as_ref()),
        })
    }
}

#[cfg(target_os = "linux")]
impl Archiver for ExtTarArchiver<'_> {
    fn add_file(&mut self, target: &Path, source: &Path) -> Result<(), MigError> {
        debug!(
            "ExtTarArchiver::add_file: '{}' , '{}'",
            target.display(),
            source.display()
        );
        if let Some(parent_dir) = target.parent() {
            let parent_dir = path_append(&self.tmp_dir, parent_dir);
            if !dir_exists(&parent_dir).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to access directory '{}'", parent_dir.display()),
            ))? {
                debug!(
                    "ExtTarArchiver::add_file: create directory '{}'",
                    parent_dir.display()
                );
                create_dir_all(&parent_dir).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("Failed to create directory '{}'", parent_dir.display()),
                ))?;
            }
        }

        let lnk_target = path_append(&self.tmp_dir, &target);

        debug!(
            "ExtTarArchiver::add_file: link '{}' to '{}'",
            source.display(),
            lnk_target.display()
        );

        symlink(source, &lnk_target).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Failed to link '{}' to '{}'",
                source.display(),
                lnk_target.display()
            ),
        ))?;
        Ok(())
    }

    fn finish(&mut self) -> Result<(), MigError> {
        let cmd_res = self
            .cmds
            .call(
                TAR_CMD,
                &[
                    "-h",
                    "-czf",
                    BACKUP_FILE,
                    "-C",
                    &*self.tmp_dir.to_string_lossy(),
                    ".",
                ],
                true,
            )
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to create backup archive '{}'",
                    self.archive.display()
                ),
            ))?;

        if !cmd_res.status.success() {
            error!(
                "Failed to create archive in '{}', message: '{}'",
                self.archive.display(),
                cmd_res.stderr
            );
            return Err(MigError::displayed());
        }

        if let Err(why) = remove_dir_all(&self.tmp_dir) {
            warn!(
                "Failed to delete temporary directory '{}' error: {:?}",
                self.tmp_dir.display(),
                why
            );
        }

        Ok(())
    }
}

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
                    archive_dir(
                        &source_path,
                        &path_append(&target_path, &source_file),
                        archiver,
                        &filter,
                    )?;
                } else {
                    if let Some(filter) = filter {
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

#[cfg(target_os = "linux")]
pub(crate) fn create_ext<'a>(
    cmds: &'a EnsuredCmds,
    file: &Path,
    config: &[VolumeConfig],
) -> Result<bool, MigError> {
    if config.len() > 0 {
        debug!("creating new backup in '{}", file.display());
        let mut archiver = ExtTarArchiver::new(cmds, file)?;
        create_int(&mut archiver, config)
    } else {
        info!("The backup configuration was empty - nothing backed up");
        Ok(false)
    }
}

pub(crate) fn create(file: &Path, config: &[VolumeConfig]) -> Result<bool, MigError> {
    if config.len() > 0 {
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

    for ref volume in config {
        info!("backup to volume: '{}'", volume.volume);

        for item in &volume.items {
            let item_src =
                PathBuf::from(&item.source)
                    .canonicalize()
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

    Ok(written)
}
