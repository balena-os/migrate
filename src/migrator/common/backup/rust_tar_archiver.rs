use crate::common::{backup::archiver::Archiver, MigErrCtx, MigError, MigErrorKind};
use failure::ResultExt;
use flate2::{write::GzEncoder, Compression};
use std::fs::File;
use std::path::Path;
use tar::Builder;

pub(crate) struct RustTarArchiver {
    archive: Builder<GzEncoder<File>>,
}

// use rust internal tar / gzip for archiving

impl RustTarArchiver {
    pub fn new<P: AsRef<Path>>(file: P) -> Result<RustTarArchiver, MigError> {
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
