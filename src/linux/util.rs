use regex::Regex;
use std::io::Read;
use std::fs::File;
use failure::{Fail,ResultExt};
use log::{trace};

const MODULE: &str = "Linux::util";

use crate::{MigError,MigErrCtx,MigErrorKind};

pub fn parse_file(fname: &str, regex: &Regex) -> Result<String,MigError> {
    let mut os_info = String::new();
    File::open(fname).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("File open '{}'",fname)))?
        .read_to_string(&mut os_info).context(MigErrCtx::from_remark(MigErrorKind::Upstream, "File read '/etc/os-release'"))?;
    
    for line in os_info.lines() {
        trace!("{}::parse_file: line: '{}'", MODULE, line);

        if let Some(cap) = regex.captures(line) {                        
            return Ok(String::from(cap.get(1).unwrap().as_str()));
        };
    }

    Err(MigError::from(MigErrorKind::NotFound))
}