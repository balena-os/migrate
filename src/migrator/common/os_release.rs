use failure::ResultExt;
use lazy_static::lazy_static;
use regex::Regex;
use std::fmt::{self, Display, Formatter};

use crate::common::{MigErrCtx, MigError, MigErrorKind};

const OS_RELEASE_RE: &str = r"^(\d+)\.(\d+)\.(\d+)(-.*)?$";

#[derive(Debug)]
pub struct OSRelease(u32, u32, u32);

impl OSRelease {
    pub fn get_mayor(&self) -> u32 {
        self.0
    }

    pub fn get_minor(&self) -> u32 {
        self.1
    }

    pub fn get_build(&self) -> u32 {
        self.2
    }

    pub fn parse_from_str(os_release: &str) -> Result<OSRelease, MigError> {
        lazy_static! {
            static ref RE_OS_VER: Regex = Regex::new(OS_RELEASE_RE).unwrap();
        }

        let captures = match RE_OS_VER.captures(os_release) {
            Some(c) => c,
            None => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                    "OSRelease::parse_from_str: parse regex failed to parse release string: '{}'",
                    os_release
                ),
                ));
            }
        };

        let parse_capture = |i: usize| -> Result<u32, MigError> {
            match captures.get(i) {
                Some(s) => Ok(s.as_str().parse::<u32>().context(MigErrCtx::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "OSRelease::parse_from_str: failed to parse {} part {} to u32",
                        os_release, i
                    ),
                ))?),
                None => {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!(
                            "OSRelease::parse_from_str: failed to get release part {} from: '{}'",
                            i, os_release
                        ),
                    ));
                }
            }
        };

        if let Ok(n0) = parse_capture(1) {
            if let Ok(n1) = parse_capture(2) {
                if let Ok(n2) = parse_capture(3) {
                    return Ok(OSRelease(n0, n1, n2));
                }
            }
        }
        Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!(
                "OSRelease::parse_from_str: failed to parse release string: '{}'",
                os_release
            ),
        ))
    }
}

impl Display for OSRelease {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}.{}.{}", self.0, self.1, self.2)
    }
}
