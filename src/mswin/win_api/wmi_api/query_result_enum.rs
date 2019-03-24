use winapi::{
    shared::{
        ntdef::NULL,        
    },    
    um::{
        wbemcli::{  IWbemClassObject,                    
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
        debug!("{}::drop: dropping IEnumWbemClassObject", MODULE);
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

