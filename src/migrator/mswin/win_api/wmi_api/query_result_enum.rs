use winapi::{
    shared::ntdef::NULL,
    um::wbemcli::{IWbemClassObject, WBEM_INFINITE},
};

use log::debug;
use std::io::Error;

use super::{IWbemClassWrapper, PMIEnumWbemClassObject};
use crate::migrator::MigError;

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
            (*self.p_enumerator).Next(WBEM_INFINITE as i32, 1, &mut pcls_obj, &mut return_value)
        };

        // TODO: figure out how to use WBEMSTATUS::WBEM_S_NO_ERROR

        if res != 0 {
            // TODO: detect 'normal' end of Enumerator
            let os_err = Error::last_os_error();
            debug!(
                "{}::next: Enumerator::Next returned {}, os_error: {:?}",
                MODULE, res, os_err
            );
            return None;
            //return Some(Err(report_win_api_error(MODULE, "next", "IEnumWbemClassObject::Next")))
        }

        debug!(
            "{}::next: Enumerator::Next returned {}, retun_value {} pcls_obj {:?}",
            MODULE, res, return_value, pcls_obj
        );

        if return_value == 0 {
            return None;
        }

        debug!(
            "{}::next: Got enumerator {:?} and obj {:?}",
            MODULE, self.p_enumerator, pcls_obj
        );

        let pcls_wrapper = IWbemClassWrapper::new(pcls_obj);

        Some(Ok(pcls_wrapper))
    }
}
