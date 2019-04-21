use failure::Fail;
use lazy_static::lazy_static;
use log::{debug, warn};
use std::io::Error;
use std::ptr::null_mut;
use std::sync::{Arc, Mutex};

use winapi::{
    shared::{
        ntdef::NULL,
        rpcdce::{RPC_C_AUTHN_LEVEL_DEFAULT, RPC_C_IMP_LEVEL_IMPERSONATE},
    },
    um::{
        combaseapi::{CoInitializeEx, CoInitializeSecurity, CoUninitialize},
        objbase::COINIT_MULTITHREADED,
        objidl::EOAC_NONE,
    },
};

use crate::migrator::{MigErrCtx, MigError, MigErrorKind};
// use super::util::{check_hres};

const MODULE: &str = "mswin::win_api::com_api";

type RefCount = Arc<Mutex<u64>>;

#[derive(Debug)]
pub struct ComAPI {
    uc: RefCount,
}

impl ComAPI {
    pub fn get_api() -> Result<ComAPI, MigError> {
        debug!("{}::new: entered", MODULE);
        lazy_static! {
            static ref COM_REF: RefCount = Arc::new(Mutex::new(0));
        }
        if let Ok(mut use_count) = COM_REF.lock() {
            if *use_count == 0 {
                debug!("{}::get_api: initializing com", MODULE);
                if unsafe { CoInitializeEx(null_mut(), COINIT_MULTITHREADED) } < 0 {
                    let os_err = Error::last_os_error();
                    warn!(
                        "{}::get_api: CoInitializeEx returned os error: {:?} ",
                        MODULE, os_err
                    );
                    return Err(MigError::from(os_err.context(MigErrCtx::from_remark(
                        MigErrorKind::WinApi,
                        &format!("{}::get_api: CoInitializeEx failed", MODULE),
                    ))));
                }
                debug!("{}::get_api: calling CoInitializeSecurity", MODULE);
                if unsafe {
                    CoInitializeSecurity(
                        NULL,
                        -1, // let COM choose.
                        null_mut(),
                        NULL,
                        RPC_C_AUTHN_LEVEL_DEFAULT,
                        RPC_C_IMP_LEVEL_IMPERSONATE,
                        NULL,
                        EOAC_NONE,
                        NULL,
                    )
                } < 0
                {
                    let os_err = Error::last_os_error();
                    unsafe { CoUninitialize() };
                    warn!(
                        "{}::get_api: CoInitializeSecurity returned os error: {:?} ",
                        MODULE, os_err
                    );
                    return Err(MigError::from(os_err.context(MigErrCtx::from_remark(
                        MigErrorKind::WinApi,
                        &format!("{}::get_api: CoInitializeSecurity failed", MODULE),
                    ))));
                }
            }
            *use_count += 1;
            Ok(ComAPI {
                uc: COM_REF.clone(),
            })
        } else {
            Err(MigError::from(MigErrorKind::MutAccess))
        }
    }
}

impl Drop for ComAPI {
    fn drop(&mut self) {
        debug!("{}::drop: called", MODULE);
        if let Ok(mut v) = self.uc.lock() {
            if *v == 1 {
                debug!("{}::drop: deinitializing com", MODULE);
                unsafe { CoUninitialize() };
            }
            *v -= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ComAPI;
    #[test]
    fn it_works1() {
        {
            let _h_com_api = ComAPI::get_api().unwrap();
        }
        {
            let _h_com_api = ComAPI::get_api().unwrap();
        }
    }
}
