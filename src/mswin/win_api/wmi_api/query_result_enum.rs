use std::ptr::{self, null_mut};
use winapi::{
    shared::{
        ntdef::NULL,
        rpcdce::{
            RPC_C_AUTHN_LEVEL_CALL, RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE,
            RPC_C_IMP_LEVEL_IMPERSONATE,
        },
        wtypesbase::CLSCTX_INPROC_SERVER,
    },    
    um::{
        oaidl::SAFEARRAY,
        objidl::EOAC_NONE,
        combaseapi::{
                    CoCreateInstance, 
                    CoSetProxyBlanket
                    },                
        wbemcli::{  IEnumWbemClassObject,
                    IWbemClassObject,
                    CLSID_WbemLocator, 
                    IID_IWbemLocator, 
                    IWbemLocator, 
                    IWbemServices, 
                    WBEM_FLAG_FORWARD_ONLY, 
                    WBEM_FLAG_RETURN_IMMEDIATELY,
                    WBEM_FLAG_ALWAYS, 
                    WBEM_FLAG_NONSYSTEM_ONLY,
                    WBEM_INFINITE,
                    },
    },
};

use log::{debug};

use crate::mig_error::{MigError};
use crate::mswin::win_api::util::report_win_api_error;
use super::{PMIEnumWbemClassObject, IWbemClassWrapper};

const MODULE: &str = "mswin::win_api::wmi_api::query_result_enum";

pub struct QueryResultEnumerator {    
    p_enumerator: PMIEnumWbemClassObject,
}

impl QueryResultEnumerator {
    pub fn new(p_enumerator: PMIEnumWbemClassObject) -> Self {
        Self {
            p_enumerator: p_enumerator,
        }
    }
}

impl<'a> Drop for QueryResultEnumerator {
    fn drop(&mut self) {        
        unsafe {
            (*self.p_enumerator).Release();
        }
    }
}

impl Iterator for QueryResultEnumerator {
    type Item = Result<IWbemClassWrapper, MigError>;

    fn next(&mut self) -> Option<Self::Item> {
        let mut pcls_obj = NULL as *mut IWbemClassObject;
        let mut return_value = 0;

        let res = unsafe {
            (*self.p_enumerator).Next(
                WBEM_INFINITE as i32,
                1,
                &mut pcls_obj,
                &mut return_value,
            ) };
        
        if res < 0 {
            return Some(Err(report_win_api_error(MODULE, "next", "IEnumWbemClassObject::Next")))
       }
        
        if return_value == 0 {
            return None;
        }

        debug!(
            "Got enumerator {:?} and obj {:?}", self.p_enumerator, pcls_obj
        );

        let pcls_wrapper = IWbemClassWrapper::new(pcls_obj);

        Some(Ok(pcls_wrapper))
    }
}

