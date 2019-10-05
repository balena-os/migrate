use digest::Digest;
use failure::ResultExt;
use log::{debug, trace};
use md5::Md5;
use mod_logger::Logger;
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::common::{MigErrCtx, MigError, MigErrorKind};

const BUFFER_SIZE: usize = 1024 * 1024;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub(crate) enum HashInfo {
    #[serde(rename = "sha1")]
    Sha1(String),
    #[serde(rename = "md5")]
    Md5(String),
}

pub(crate) fn check_digest<P: AsRef<Path>>(path: P, digest: &HashInfo) -> Result<bool, MigError> {
    //let path= path.as_ref();
    trace!(
        "check_digest entered with '{}', {:?}",
        path.as_ref().display(),
        digest
    );
    let computed = match digest {
        HashInfo::Sha1(_) => HashInfo::Sha1(process_digest::<Sha1, _>(path)?),
        HashInfo::Md5(_) => HashInfo::Md5(process_digest::<Md5, _>(path)?),
    };

    debug!("check_digest: provided digest is: {:?}", digest);
    debug!("check_digest: computed digest is: {:?}", computed);
    Ok(computed == *digest)
}

pub(crate) fn get_default_digest<P: AsRef<Path>>(path: P) -> Result<HashInfo, MigError> {
    Ok(HashInfo::Md5(process_digest::<Md5, _>(path)?))
}

fn process_digest<D: Digest + Default, P: AsRef<Path>>(path: P) -> Result<String, MigError> {
    trace!(
        "process_digest entered with file: '{}'",
        path.as_ref().display()
    );

    let path = path.as_ref();
    let mut file = File::open(path).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("Failed to open file '{}'", path.display()),
    ))?;
    trace!("file opened");

    let mut sh = D::default();

    let mut buffer_vec: Vec<u8> = vec![0; BUFFER_SIZE];
    let buffer = buffer_vec.as_mut_slice();
    trace!("buffer created, entering loop");

    loop {
        let n = file.read(buffer).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to read from file '{}'", path.display()),
        ))?;
        sh.input(&buffer[..n]);
        if n == 0 || n < BUFFER_SIZE {
            break;
        }
    }
    trace!("loop has terminated");
    let digest = sh.result();
    trace!("got digest");
    let mut res = String::from("");
    for byte in &digest {
        res.push_str(&format!("{:02x}", byte));
    }
    trace!("done");
    Ok(res)
}
