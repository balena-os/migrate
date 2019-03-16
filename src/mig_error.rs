
use failure::{Error,Fail,Backtrace,ResultExt,Context};
use std::fmt::{self,Debug,Display,Formatter};

#[derive(Copy, Clone, Eq, PartialEq, Debug, Fail)]
pub enum MigErrorKind {
    #[fail(display = "An error occured in an upstream function")]
    Upstream,
    #[fail(display = "An item could not be found")]
    FileRead,    
    #[fail(display = "An unknown error occurred")]
    Unknown,
    #[fail(display = "The OS type is not supported")]
    InvOSType,
    #[fail(display = "The function has not been implemented yet")]
    NotImpl,
    #[fail(display = "A command IO stream operation failed")]
    CmdIO,
    #[fail(display = "An invalid value was encountered")]
    InvParam,
    #[fail(display = "A required program could not be found")]
    PgmNotFound,
    #[fail(display = "A required item could not be found")]
    NotFound,
    #[fail(display = "A required feature is not available")]
    FeatureMissing,
    #[fail(display = "Initialization of WMI")]    
    WmiInit,    
    #[fail(display = "A WMI query failed")]    
    WmiQueryFailed,    
}

pub struct MigErrCtx {
    kind: MigErrorKind,
    descr: String,
}

impl MigErrCtx {
    pub fn from_remark(kind: MigErrorKind, descr: &str) -> MigErrCtx {
        MigErrCtx{kind,descr: String::from(descr)}
    }
}

impl From<MigErrorKind> for MigErrCtx {
    fn from(kind: MigErrorKind) -> MigErrCtx {
        MigErrCtx{kind, descr: String::new()}
    }
}

impl Display for MigErrCtx {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if self.descr.is_empty() {
            write!(f, "Error: {}", self.kind)
        } else {
            write!(f, "Error: {}, {}", self.kind, self.descr)
        }
    }
}

#[derive(Debug)]
pub struct MigError {
    inner: Context<MigErrCtx>,
}


impl Fail for MigError {
    fn name(&self) -> Option<&str> {
        self.inner.name()
    }

    fn cause(&self) -> Option<&Fail> {
        self.inner.cause()
    }

    fn backtrace(&self) -> Option<&Backtrace> {
        self.inner.backtrace()
    }
}

impl Display for MigError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Display::fmt(&self.inner, f)
    }
}

impl MigError {
    pub fn kind(&self) -> MigErrorKind {
        self.inner.get_context().kind
    }

    pub fn from_remark(kind: MigErrorKind, remark: &str) -> MigError {
        MigError { inner: Context::new(MigErrCtx::from_remark(kind, remark)) } 
    }

}

impl From<MigErrCtx> for MigError {
    fn from(mig_ctxt: MigErrCtx) -> MigError {
        MigError { inner: Context::new(mig_ctxt) }
    }
}

impl From<Context<MigErrCtx>> for MigError {
    fn from(inner: Context<MigErrCtx>) -> MigError {
        MigError { inner: inner }
    }
}
