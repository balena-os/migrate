use lazy_static::lazy_static;
use std::sync::{Mutex,Arc};
use std::ptr::null_mut;
use log::{trace,warn};
use failure::{Fail,ResultExt};
use std::io::Error;


use winapi::{
    shared::{
        ntdef::NULL,
        rpcdce::{
            RPC_C_AUTHN_LEVEL_DEFAULT,RPC_C_IMP_LEVEL_IMPERSONATE,
        },
    },
    um::{
        combaseapi::{
            CoInitializeEx, CoInitializeSecurity, CoSetProxyBlanket, CoUninitialize,
        },
        objbase::COINIT_MULTITHREADED,
        objidl::EOAC_NONE,
    },
};

use crate::{MigError, MigErrorKind, MigErrCtx};
// use super::util::{check_hres};

const MODULE: &str = "mswin::win_api::com_api";

pub struct ComAPI {  }

impl ComAPI {
    // try to make sure ComInialize is being called once only
    // TODO: take care of deinitialization later
    pub fn get_api() -> Result<Arc<Mutex<ComAPI>>,MigError> {
        lazy_static! {
            static ref COM_REF: Arc<Mutex<ComAPI>> = Arc::new(Mutex::new(ComAPI{}));             
        }
        
        if let Ok(_mg) = (*COM_REF).lock() {
            if Arc::strong_count(&*COM_REF) == 1 {
                trace!("{}::get_api: initializing com", MODULE);
                if unsafe { CoInitializeEx(null_mut(), COINIT_MULTITHREADED) } < 0 {
                    let os_err = Error::last_os_error();
                    warn!("{}::get_api: CoInitializeEx returned os error: {:?} ", MODULE, os_err);       
                    return Err(
                        MigError::from(
                            os_err.context(
                                MigErrCtx::from_remark(MigErrorKind::WinApi, &format!("{}::get_api: CoInitializeEx failed",MODULE)))));
                }
                if unsafe { 
                    CoInitializeSecurity(
                        NULL,
                        -1, // let C    OM choose.
                        null_mut(),
                        NULL,
                        RPC_C_AUTHN_LEVEL_DEFAULT,
                        RPC_C_IMP_LEVEL_IMPERSONATE,
                        NULL,
                        EOAC_NONE,
                        NULL,) } < 0 {                    
                    let os_err = Error::last_os_error();
                    unsafe { CoUninitialize() };                                        
                    warn!("{}::get_api: CoInitializeSecurity returned os error: {:?} ", MODULE, os_err);       
                    return Err(
                        MigError::from(
                            os_err.context(
                                MigErrCtx::from_remark(MigErrorKind::WinApi, &format!("{}::get_api: CoInitializeSecurity failed",MODULE)))));                    
                    }            
            }

            Ok(COM_REF.clone())
        } else {
            Err(MigError::from_remark(MigErrorKind::MutAccess, &format!("{}::get_api: failed to lock mutex",MODULE)))
        }        
    }
}


