use std::{error::Error, fmt};

#[derive(Debug)]
pub enum MigErrorCode{
    ErrUnknown,
    ErrInvOSType,
    ErrNotImpl,
    ErrExecProcess,
    ErrCmdIO,
    ErrInvParam,
}

#[derive(Debug)]
pub struct MigError{
    code: MigErrorCode,
    msg: String,
    source: Option<Box<Error>>,
}

impl MigError {
    pub fn from_code(code: MigErrorCode, msg: &str, source: Option<Box<Error>>) -> MigError {
        MigError{code, msg: String::from(msg), source}
    }
}

impl Error for MigError {}

impl fmt::Display for MigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Error: {:?} - {} ", self.code, self.msg )
    }
}