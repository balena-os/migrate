use std::{error::Error, fmt};

#[derive(Debug)]
pub enum MigErrorCode{
    ErrUnknown(String),
    ErrInvOSType(String),
    ErrNotImpl(String)
}

#[derive(Debug)]
pub struct MigError{
    code: MigErrorCode,
}

impl MigError {
    pub fn from_code(code: MigErrorCode) -> MigError {
        MigError{code}
    }
}

impl Error for MigError {}

impl fmt::Display for MigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Oh no, something bad went down")
    }
}