use crate::common::MigError;

pub(crate) trait ImageFile {
    fn fill(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), MigError>;
}
