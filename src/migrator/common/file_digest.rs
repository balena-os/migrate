use digest::Digest;
use failure::ResultExt;
use md5::Md5;
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use crate::common::{MigErrCtx, MigError, MigErrorKind};

const BUFFER_SIZE: usize = 1024 * 1024;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub(crate) enum HashInfo {
    Sha1(String),
    Md5(String),
}

pub(crate) fn check_digest(path: &PathBuf, digest: &HashInfo) -> Result<bool, MigError> {
    let computed = match digest {
        HashInfo::Sha1(_) => HashInfo::Sha1(process_digest::<Sha1>(path)?),
        HashInfo::Md5(_) => HashInfo::Sha1(process_digest::<Md5>(path)?),
    };
    Ok(computed == *digest)
}

/*
pub (crate) fn get_digest(path: &PathBuf, hash_type: &str ) -> Result<HashInfo, MigError> {
    match hash_type.to_ascii_lowercase().as_ref() {
        "sha1" => Ok(HashInfo::Sha1(process_digest::<Sha1>(path)?)),
        "md5" => Ok(HashInfo::Md5(process_digest::<Md5>(path)?)),
        _ => Err(MigError::from_remark(MigErrorKind::InvParam, &format!("Invalid/unsupported digest type encountered: '{}'",  hash_type)))
    }
}
*/

pub(crate) fn get_default_digest(path: &PathBuf) -> Result<HashInfo, MigError> {
    Ok(HashInfo::Md5(process_digest::<Md5>(path)?))
}

fn process_digest<D: Digest + Default>(path: &PathBuf) -> Result<String, MigError> {
    let mut file = File::open(path).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("Failed to open file '{}'", path.display()),
    ))?;

    let mut sh = D::default();
    let mut buffer = [0u8; BUFFER_SIZE];
    loop {
        let n = file.read(&mut buffer).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to read from file '{}'", path.display()),
        ))?;
        sh.input(&buffer[..n]);
        if n == 0 || n < BUFFER_SIZE {
            break;
        }
    }
    let digest = sh.result();
    let mut res = String::from("");
    for byte in &digest {
        res.push_str(&format!("{:02x}", byte));
    }
    Ok(res)
}
