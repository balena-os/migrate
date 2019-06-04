// extern crate winapi;
use failure::Fail;
use log::{debug, warn};
use std::ffi::OsStr;
use std::io::Error;
use std::iter::once;
use std::os::windows::prelude::*;
use std::ptr::null_mut;

use winapi::shared::winerror::ERROR_INVALID_FUNCTION;
use winapi::um::fileapi::{FindFirstVolumeW, FindNextVolumeW, FindVolumeClose, QueryDosDeviceW};
use winapi::um::handleapi::INVALID_HANDLE_VALUE;
use winapi::um::winbase::GetFirmwareEnvironmentVariableW;

use crate::common::{MigErrCtx, MigError, MigErrorKind};

/* for testing
pub(crate)mod util;
pub(crate) mod com_api;
pub(crate) mod wmi_api;
*/

pub mod com_api;
pub mod util;
pub mod wmi_api;

use util::{clip, to_string, to_string_list};

const MODULE: &str = "mswin::win_api";

fn get_volumes() -> Result<Vec<String>, MigError> {
    debug!("{}::get_volumes: entered", MODULE);
    const BUFFER_SIZE: usize = 2048;
    let mut buffer: [u16; BUFFER_SIZE] = [0; BUFFER_SIZE];
    let mut vol_list: Vec<String> = Vec::new();

    let h_search = unsafe { FindFirstVolumeW(buffer.as_mut_ptr(), BUFFER_SIZE as u32) };

    if h_search == INVALID_HANDLE_VALUE {
        return Err(MigError::from(
            Error::last_os_error().context(MigErrCtx::from(MigErrorKind::WinApi)),
        ));
    }

    vol_list.push(to_string(&buffer)?);

    loop {
        let ret = unsafe { FindNextVolumeW(h_search, buffer.as_mut_ptr(), BUFFER_SIZE as u32) };
        if ret == 0 {
            unsafe { FindVolumeClose(h_search) };
            return Ok(vol_list);
        }
        vol_list.push(to_string(&buffer)?);
    }
}

pub fn query_dos_device(dev_name: Option<&str>) -> Result<Vec<String>, MigError> {
    debug!("{}::query_dos_device: entered with {:?}", MODULE, dev_name);
    match dev_name {
        Some(s) => {
            const BUFFER_SIZE: usize = 8192;
            let mut buffer: [u16; BUFFER_SIZE] = [0; BUFFER_SIZE];
            let dev_path: Vec<u16> = OsStr::new(&s).encode_wide().chain(once(0)).collect();
            let num_tchar = unsafe {
                QueryDosDeviceW(dev_path.as_ptr(), buffer.as_mut_ptr(), BUFFER_SIZE as u32)
            };
            if num_tchar > 0 {
                debug!("{}::query_dos_device: success", MODULE);
                Ok(to_string_list(&buffer)?)
            } else {
                let os_err = Error::last_os_error();
                warn!(
                    "{}::query_dos_device: returned {}, last os error: {:?} ",
                    MODULE, num_tchar, os_err
                );
                return Err(MigError::from(
                    os_err.context(MigErrCtx::from(MigErrorKind::WinApi)),
                ));
            }
        }
        None => {
            const BUFFER_SIZE: usize = 32768;
            let mut buffer: [u16; BUFFER_SIZE] = [0; BUFFER_SIZE];
            let num_tchar =
                unsafe { QueryDosDeviceW(null_mut(), buffer.as_mut_ptr(), BUFFER_SIZE as u32) };
            if num_tchar > 0 {
                debug!("{}::query_dos_device: success", MODULE);
                Ok(to_string_list(&buffer)?)
            } else {
                let os_err = Error::last_os_error();
                warn!(
                    "{}::query_dos_device: returned {}, last os error: {:?} ",
                    MODULE, num_tchar, os_err
                );
                return Err(MigError::from(
                    os_err.context(MigErrCtx::from(MigErrorKind::WinApi)),
                ));
            }
        }
    }
}

pub(crate) fn is_efi_boot() -> Result<bool, MigError> {
    // TODO: only works on windows 10 and upwards
    // TODO: alt - try to mount efi drive ?

    let dummy: Vec<u16> = OsStr::new("").encode_wide().chain(once(0)).collect();
    let guid: Vec<u16> = OsStr::new("{00000000-0000-0000-0000-000000000000}")
        .encode_wide()
        .chain(once(0))
        .collect();
    let res =
        unsafe { GetFirmwareEnvironmentVariableW(dummy.as_ptr(), guid.as_ptr(), null_mut(), 0) };

    if res != 0 {
        return Err(MigError::from_remark(
            MigErrorKind::InvParam,
            "is_uefi_boot: received no error where an error was expected",
        ));
    }

    let os_err = Error::last_os_error();

    match os_err.raw_os_error() {
        Some(err) => {
            // ERROR_INVALID_FUNCTION tells us that the function is not available / NON UEFI
            if err == ERROR_INVALID_FUNCTION as i32 {
                Ok(false)
            } else {
                // Other errors indicate UEFI system
                debug!("is_uefi_boot: error value: {}", err);
                Ok(true)
            }
        }
        None => Err(MigError::from_remark(
            MigErrorKind::InvParam,
            &format!("is_uefi_boot: no error where an error was expected",),
        )),
    }
}

pub fn enumerate_volumes() -> Result<i32, MigError> {
    match query_dos_device(None) {
        Ok(sl) => {
            for device in sl {
                println!("got device name: {}", device);
            }
        }
        Err(why) => {
            println!("query_dos_device retured error: {:?}", why);
        }
    };

    for vol_name in get_volumes()? {
        let dev_name = clip(&vol_name, Some("\\\\?\\"), Some("\\"));

        println!("got dev_name: {}", dev_name);

        for device in query_dos_device(Some(dev_name))? {
            println!("  got dev_name: {}", device);
        }
    }

    Ok(0)
}
