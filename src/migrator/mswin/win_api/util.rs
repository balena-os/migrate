use crate::{MigErrCtx, MigError, MigErrorKind};
use failure::ResultExt;
use std::ffi::{OsStr};
use std::iter::once;
use std::os::windows::prelude::*;
use log::{warn};
use std::io::Error;
use failure::{Fail};

pub fn to_string(os_str_buf: &[u16]) -> Result<String, MigError> {
    match os_str_buf.iter().position(|&x| x == 0) {
        Some(i) => Ok(String::from_utf16_lossy(&os_str_buf[0..i])),
        None => return Err(MigError::from(MigErrorKind::InvParam)),
    }
}

pub fn to_string_list(os_str_buf: &[u16]) -> Result<Vec<String>, MigError> {
    let mut str_list: Vec<String> = Vec::new();
    let mut start: usize = 0;
    for curr in os_str_buf.iter().enumerate() {
        if *curr.1 == 0 {
            if start < curr.0 {
                let s = to_string(&os_str_buf[start..curr.0 + 1])
                    .context(MigErrCtx::from(MigErrorKind::InvParam))?;
                str_list.push(s);
                start = curr.0 + 1;
            } else {
                break;
            }
        }
    }
    Ok(str_list)
}

pub fn clip<'a>(clip_str: &'a str, clip_start: Option<&str>, clip_end: Option<&str>) -> &'a str {
    let mut work_str = clip_str;

    if let Some(s) = clip_start {
        if !s.is_empty() && work_str.starts_with(s) {
            work_str = &work_str[s.len()..];
        }
    }

    if let Some(s) = clip_end {
        if !s.is_empty() && work_str.ends_with(s) {
            work_str = &work_str[0..(work_str.len() - s.len())];
        }
    }

    work_str
}

pub fn to_wide_cstring(str: &str) -> Vec<u16> {
    OsStr::new(str).encode_wide().chain(once(0)).collect()
}

pub fn report_win_api_error(module: &str, func: &str, called: &str) -> MigError {
    let os_err = Error::last_os_error();

    warn!(
        "{}::{}: {} returned os error: {:?} ",
        module, func, called, os_err
    );

    MigError::from(
        os_err.context(MigErrCtx::from_remark(
            MigErrorKind::WinApi,
            &format!("{}::{}: {} failed", module, func, called),)))
}

pub fn report_win_api_error_with_deinit<T: Fn() -> ()>(module: &str, func: &str, called: &str, deinit: T) -> MigError {
    let os_err = Error::last_os_error();

    warn!(
        "{}::{}: {} returned os error: {:?} ",
        module, func, called, os_err
    );

    deinit();

    MigError::from(
        os_err.context(MigErrCtx::from_remark(
            MigErrorKind::WinApi,
            &format!("{}::{}: {} failed", module, func, called),)))
}