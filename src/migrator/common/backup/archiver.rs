use crate::common::MigError;
use std::path::Path;

pub trait Archiver {
    fn add_file(&mut self, target: &Path, source: &Path) -> Result<(), MigError>;
    fn finish(&mut self) -> Result<(), MigError>;
}
