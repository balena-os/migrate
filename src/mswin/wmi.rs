extern crate winapi;

use crate::common::mig_error::{MigError,MigErrorCode};

use log::{trace};

use std::ptr::null_mut;
use winapi::um::combaseapi::{CoInitializeSecurity, CoUninitialize};
use winapi::um::objbase::{CoInitialize};
use winapi::um::handleapi::{INVALID_HANDLE_VALUE};
use winapi::um::objidl::{EOAC_NONE};
use lazy_static::lazy_static;

const MODULE: &str = "wmi";

const S_OK: i32 = 0;
// TODO: retrieve definitions
const RPC_C_AUTHN_LEVEL_DEFAULT: u32 = 0;
const RPC_C_IMP_LEVEL_IMPERSONATE: u32 = 3;

pub struct ComHandle {
    h_com: i32,
}


impl ComHandle {
    pub fn try_init() -> Result<ComHandle,MigError> {
        trace!("{}::try_init: entered",MODULE);

        let h_com = unsafe { CoInitialize(null_mut()) };
        if h_com != S_OK {
            return Err(MigError::from_code(MigErrorCode::ErrNotImpl, &format!("{}::try_init: failed to initialize COM interface: rc={}",MODULE,h_com), None));    
        } 

        trace!("{}::try_init: com initialized, init security",MODULE);
        let res =  unsafe { CoInitializeSecurity(
                null_mut(),                 // Security descriptor    
                -1,                         // COM negotiates authentication service
                null_mut(),                 // Authentication services
                null_mut(),                 // Reserved
                RPC_C_AUTHN_LEVEL_DEFAULT,  // Default authentication level for proxies
                RPC_C_IMP_LEVEL_IMPERSONATE,// Default Impersonation level for proxies
                null_mut(),                 // Authentication info
                EOAC_NONE,                  // Additional capabilities of the client or server
                null_mut()) };              // Reserved

        if res != S_OK {
            unsafe { CoUninitialize() };
            return Err(MigError::from_code(MigErrorCode::ErrNotImpl, &format!("{}::try_init: failed to initialize COM interface Security: rc={}",MODULE,res), None));            
        }        

        trace!("{}::try_init: com initialized",MODULE);
        

        Ok(ComHandle{h_com: h_com})
    }
}