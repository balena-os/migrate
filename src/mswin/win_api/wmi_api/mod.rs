use failure::Fail;
use log::{debug, warn};
use std::io::Error;
use std::ptr::{self, null_mut};
use std::collections::HashMap;

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
                    },
    },
};

use super::com_api::ComAPI;
use super::util::to_wide_cstring;
use crate::{MigErrCtx, MigError, MigErrorKind};


type PMIWbemLocator = *mut IWbemLocator;
type PMIWbemServices = *mut IWbemServices;
type PMIEnumWbemClassObject = *mut IEnumWbemClassObject;
type PMIWbemClassObject = *mut IWbemClassObject;

mod variant;
pub use variant::{Variant};
mod iwbem_class_wr;
pub use iwbem_class_wr::{IWbemClassWrapper};
mod query_result_enum;
pub use query_result_enum::QueryResultEnumerator;

const MODULE: &str = "mswin::win_api::wmi_api";

// TODO: make singleton like ComAPI

#[derive(Debug)]
pub struct WmiAPI {
    _com_api: ComAPI,
    p_svc: PMIWbemServices,
}

impl<'a> WmiAPI {
    pub fn get_api() -> Result<WmiAPI, MigError> {
        WmiAPI::get_api_from_hcom(ComAPI::get_api()?)
    }

    pub fn get_api_from_hcom(h_com_api: ComAPI) -> Result<WmiAPI, MigError> {
        debug!("{}::get_api_from_hcom: Calling CoCreateInstance for CLSID_WbemLocator", MODULE);

        let mut p_loc = NULL;

        if unsafe {
            CoCreateInstance(
                &CLSID_WbemLocator,
                null_mut(),
                CLSCTX_INPROC_SERVER,
                &IID_IWbemLocator,
                &mut p_loc,
            )
        } < 0
        {
            let os_err = Error::last_os_error();
            warn!(
                "{}::get_api_from_hcom: CoCreateInstance returned os error: {:?} ",
                MODULE, os_err
            );
            return Err(MigError::from(os_err.context(MigErrCtx::from_remark(
                MigErrorKind::WinApi,
                &format!("{}::get_api_from_hcom: CoCreateInstance failed", MODULE),
            ))));
        }

        debug!("{}::get_api_from_hcom: Got locator {:?}", MODULE, p_loc);

        debug!("{}::get_api_from_hcom: Calling ConnectServer", MODULE);

        let mut p_svc = null_mut::<IWbemServices>();

        if unsafe {
            (*(p_loc as PMIWbemLocator)).ConnectServer(
                to_wide_cstring("ROOT\\CIMV2").as_ptr() as *mut _,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut p_svc,
            )
        } < 0
        {
            let os_err = Error::last_os_error();
            warn!(
                "{}::get_api_from_hcom: ConnectServer returned os error: {:?} ",
                MODULE, os_err
            );
            return Err(MigError::from(os_err.context(MigErrCtx::from_remark(
                MigErrorKind::WinApi,
                &format!("{}::get_api_from_hcom: ConnectServer failed", MODULE),
            ))));
        }

        debug!("{}::get_api_from_hcom: Got services {:?}",MODULE, p_svc);

        let wmi_api = Self {
            _com_api: h_com_api,
            p_svc: p_svc,
        };

        debug!("{}::get_api_from_hcom: Calling CoSetProxyBlanket", MODULE);

        if unsafe {
            CoSetProxyBlanket(
                wmi_api.p_svc as _,          // Indicates the proxy to set
                RPC_C_AUTHN_WINNT,           // RPC_C_AUTHN_xxx
                RPC_C_AUTHZ_NONE,            // RPC_C_AUTHZ_xxx
                null_mut(),                  // Server principal name
                RPC_C_AUTHN_LEVEL_CALL,      // RPC_C_AUTHN_LEVEL_xxx
                RPC_C_IMP_LEVEL_IMPERSONATE, // RPC_C_IMP_LEVEL_xxx
                NULL,                        // client identity
                EOAC_NONE,                   // proxy capabilities
            )
        } < 0
        {
            let os_err = Error::last_os_error();
            warn!(
                "{}::get_api_from_hcom: CoSetProxyBlanket returned os error: {:?} ",
                MODULE, os_err
            );
            return Err(MigError::from(os_err.context(MigErrCtx::from_remark(
                MigErrorKind::WinApi,
                &format!("{}::get_api_from_hcom: CoSetProxyBlanket failed", MODULE),
            ))));
        }
        debug!("{}::get_api_from_hcom: Done", MODULE);
        Ok(wmi_api)
    }

    pub fn raw_query(&self, query: &str) -> Result<Vec<HashMap<String,Variant>>,MigError> {
        debug!("{}::raw_query: entered with {}", MODULE, query);
        let query_language = to_wide_cstring("WQL");
        let query = to_wide_cstring(query);

        let mut p_enumerator = NULL as PMIEnumWbemClassObject;

        if unsafe {
            (*self.p_svc).ExecQuery(
                query_language.as_ptr() as *mut _,
                query.as_ptr() as *mut _,
                (WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY) as i32,
                ptr::null_mut(),
                &mut p_enumerator,
            ) } != 0 {
            let os_err = Error::last_os_error();
            warn!(
                "{}::raw_query: ExecQuery returned os error: {:?} ",
                MODULE, os_err
            );
            return Err(MigError::from(os_err.context(MigErrCtx::from_remark(
                MigErrorKind::WinApi,
                &format!("{}::raw_query: ExecQuery failed", MODULE),
            ))));
        }

        debug!("{}::raw_query: Got enumerator {:?}", MODULE, p_enumerator);

        let mut result: Vec<HashMap<String,Variant>> = Vec::new();
        for iwbem_obj in QueryResultEnumerator::new(p_enumerator) {
            debug!("{}::raw_query: got object", MODULE);            
            match iwbem_obj {
                Ok(obj) => {
                    result.push(obj.to_map()?);
/*                    debug!("{}::raw_query:   is object", MODULE);                    
                    for prop in obj.list_properties()? {
                        debug!("{}::raw_query:     has property: {:?}", MODULE, prop);                    
                    }
*/                    
                }, 
                Err(why) => {
                    warn!("{}::raw_query:   is error result: {:?}", MODULE, why);                    
                }
            }
        }

        //Ok(QueryResultEnumerator::new(self, p_enumerator))

        Ok(result)
        //Err(MigError::from(MigErrorKind::NotImpl))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_wmi_api() {
        let wmi_api = WmiAPI::get_api().unwrap();
        let query_res = wmi_api.raw_query("SELECT Caption,Version,OSArchitecture, BootDevice, TotalVisibleMemorySize,FreePhysicalMemory FROM Win32_OperatingSystem").unwrap();
        assert_eq!(query_res.len(),1);
        let res_amp = query_res.get(0).unwrap();
        let caption = res_amp.get("Caption").unwrap().as_str();
        assert!(!caption.is_empty())
    }
}
