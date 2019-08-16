//pub mod mig_error;
use failure::ResultExt;
use log::debug;
use log::trace;
use regex::Regex;
use std::fs::{metadata, read_to_string, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use crate::defs::BALENA_FILE_TAG_REGEX;

pub(crate) mod mig_error;

#[cfg(target_os = "windows")]
pub(crate) mod os_release;

pub(crate) mod file_digest;

pub(crate) mod disk_util;

pub(crate) mod backup;

pub(crate) mod balena_cfg_json;
pub(crate) mod config;
// pub(crate) mod config_helper;
pub(crate) mod file_info;

pub(crate) mod stage2_config;

pub(crate) mod wifi_config;

//pub mod logger;
//pub(crate) use logger::Logger;

pub(crate) use self::config::{Config, MigMode};
pub(crate) use self::file_info::{FileInfo, FileType};
pub use self::mig_error::{MigErrCtx, MigError, MigErrorKind};

const MODULE: &str = "migrator::common";

#[derive(Debug)]
pub(crate) struct CmdRes {
    pub stdout: String,
    pub stderr: String,
    pub status: ExitStatus,
}

pub(crate) fn path_append<P1: AsRef<Path>, P2: AsRef<Path>>(base: P1, append: P2) -> PathBuf {
    let base = base.as_ref();
    let append = append.as_ref();

    if append.is_absolute() {
        let mut components = append.components();
        let mut curr = PathBuf::from(base);
        components.next();
        for comp in components {
            curr = curr.join(comp);
        }
        curr
    } else {
        base.join(append)
    }
}

pub(crate) fn file_size<P: AsRef<Path>>(file_name: P) -> Result<u64, MigError> {
    let file_name = file_name.as_ref();
    let metadata = metadata(file_name).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!(
            "failed to retrieve metadata for file: '{}'",
            file_name.display()
        ),
    ))?;

    if metadata.is_file() {
        Ok(metadata.len())
    } else {
        Ok(0)
    }
}

pub(crate) fn is_balena_file<P: AsRef<Path>>(file_name: P) -> Result<bool, MigError> {
    let path = file_name.as_ref();
    let file = File::open(path).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("failed to open file '{}'", path.display()),
    ))?;
    if let Some(ref line1) = BufReader::new(file).lines().next() {
        if let Ok(ref line1) = line1 {
            Ok(Regex::new(BALENA_FILE_TAG_REGEX).unwrap().is_match(&line1))
        } else {
            Ok(false)
        }
    } else {
        Ok(false)
    }
}

pub(crate) fn parse_file<P: AsRef<Path>>(
    fname: P,
    regex: &Regex,
) -> Result<Option<Vec<String>>, MigError> {
    let path = fname.as_ref();
    let os_info = read_to_string(path).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("File read '{}'", path.display()),
    ))?;

    for line in os_info.lines() {
        debug!("parse_file: line: '{}'", line);

        if let Some(ref captures) = regex.captures(line) {
            let mut results: Vec<String> = Vec::new();
            for cap in captures.iter() {
                if let Some(cap) = cap {
                    results.push(String::from(cap.as_str()));
                } else {
                    results.push(String::from(""));
                }
            }
            return Ok(Some(results));
        };
    }

    Ok(None)
}

pub fn dir_exists<P: AsRef<Path>>(name: P) -> Result<bool, MigError> {
    let path = name.as_ref();
    if path.exists() {
        Ok(name
            .as_ref()
            .metadata()
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "{}::dir_exists: failed to retrieve metadata for path: '{}'",
                    MODULE,
                    path.display()
                ),
            ))?
            .file_type()
            .is_dir())
    } else {
        Ok(false)
    }
}

pub fn file_exists<P: AsRef<Path>>(file: P) -> bool {
    file.as_ref().exists()
}

pub(crate) fn call(cmd: &str, args: &[&str], trim_stdout: bool) -> Result<CmdRes, MigError> {
    trace!("call(): '{}' called with {:?}, {}", cmd, args, trim_stdout);

    let output = Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "{}::call: failed to execute: command {} '{:?}'",
                MODULE, cmd, args
            ),
        ))?;

    Ok(CmdRes {
        stdout: match trim_stdout {
            true => String::from(String::from_utf8_lossy(&output.stdout).trim()),
            false => String::from(String::from_utf8_lossy(&output.stdout)),
        },
        stderr: String::from(String::from_utf8_lossy(&output.stderr)),
        status: output.status,
    })
}

pub fn check_tcp_connect(host: &str, port: u16, timeout: u64) -> Result<(), MigError> {
    use std::net::{Shutdown, TcpStream, ToSocketAddrs};
    use std::time::Duration;
    let url = format!("{}:{}", host, port);
    let mut addrs_iter = url.to_socket_addrs().context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!(
            "{}::check_tcp_connect: failed to resolve host address: '{}'",
            MODULE, url
        ),
    ))?;

    if let Some(ref sock_addr) = addrs_iter.next() {
        let tcp_stream = TcpStream::connect_timeout(sock_addr, Duration::new(timeout, 0)).context(
            MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "{}::check_tcp_connect: failed to connect to: '{}' with timeout: {}",
                    MODULE, url, timeout
                ),
            ),
        )?;

        let _res = tcp_stream.shutdown(Shutdown::Both);
        Ok(())
    } else {
        Err(MigError::from_remark(
            MigErrorKind::InvState,
            &format!(
                "{}::check_tcp_connect: no results from name resolution for: '{}",
                MODULE, url
            ),
        ))
    }
}

const GIB_SIZE: u64 = 1024 * 1024 * 1024;
const MIB_SIZE: u64 = 1024 * 1024;
const KIB_SIZE: u64 = 1024;

pub fn format_size_with_unit(size: u64) -> String {
    if size > (10 * GIB_SIZE) {
        format!("{} GiB", size / GIB_SIZE)
    } else if size > (10 * MIB_SIZE) {
        format!("{} MiB", size / MIB_SIZE)
    } else if size > (10 * KIB_SIZE) {
        format!("{} KiB", size / KIB_SIZE)
    } else {
        format!("{} B", size)
    }
}
