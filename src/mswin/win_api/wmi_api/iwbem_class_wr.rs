use failure::{Fail,ResultExt};
use log::{debug, warn};
use std::ptr::{self, null_mut};
use std::slice;
use widestring::WideCStr;

use winapi::{
    shared::{
        ntdef::NULL,
        wtypes::BSTR,
    },    
    um::{
        oaidl::SAFEARRAY,
        oleauto::{
            SafeArrayAccessData,
            SafeArrayUnaccessData,
            SafeArrayGetLBound,
            SafeArrayGetUBound,
            SafeArrayDestroy },
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
                    },
    },
};

use crate::mswin::win_api::util::{report_win_api_error};
use crate::mig_error::{MigError, MigErrorKind, MigErrCtx};

const MODULE: &str = "mswin::win_api::wmi_api::iwbem_class";

pub struct IWbemClassWrapper {
    pub inner: IWbemClassObject,
}

impl IWbemClassWrapper {
    pub fn new(ptr: IWbemClassObject) -> Self {
        Self { inner: ptr }
    }

    pub fn list_properties(&self) -> Result<Vec<String>, MigError> {
        // This will store the properties names from the GetNames call.
        let mut p_names = NULL as *mut SAFEARRAY;

        if unsafe {
            self.inner.GetNames(
                ptr::null(),
                (WBEM_FLAG_ALWAYS | WBEM_FLAG_NONSYSTEM_ONLY) as i32,
                ptr::null_mut(),
                &mut p_names,
            ) } < 0 {
                return Err(report_win_api_error(MODULE, "list_properties", "IWbemClassObject::GetNames"));
            }
        
        let result = safe_to_str_array(p_names);

        if unsafe { SafeArrayDestroy(p_names) } < 0 {
            return Err(report_win_api_error(MODULE, "list_properties", "SafeArrayDestroy"));
        }

        result
    }
}

impl Drop for IWbemClassWrapper {
    fn drop(&mut self) {
        unsafe {
            (*self.inner).Release();
        }
    }
}

fn safe_to_str_array(arr: *mut SAFEARRAY) -> Result<Vec<String>,MigError> {
    let mut p_data = NULL;
    let mut lower_bound: i32 = 0;
    let mut upper_bound: i32 = 0;

    if unsafe { SafeArrayGetLBound(arr, 1, &mut lower_bound as _) } < 0 {
        return Err(report_win_api_error(MODULE, "safe_to_str_array", "SafeArrayGetLBound"));
    }

    if unsafe { SafeArrayGetUBound(arr, 1, &mut upper_bound as _) } < 0 {
        return Err(report_win_api_error(MODULE, "safe_to_str_array", "SafeArrayGetUBound"));
    }

    if unsafe { SafeArrayAccessData(arr, &mut p_data) } < 0 {
        return Err(report_win_api_error(MODULE, "safe_to_str_array", "SafeArrayAccessData"));
    }

    let mut result: Vec<String> = Vec::new();
    let data_slice = unsafe { slice::from_raw_parts(p_data as *mut BSTR, (upper_bound + 1) as usize) };
    let data_slice = &data_slice[(lower_bound as usize)..];
    for item_bstr in data_slice.iter() {
        let item: &WideCStr = unsafe { WideCStr::from_ptr_str(*item_bstr) };
        result.push(
            String::from(
                item.to_string()
                    .context(MigErrCtx::from_remark(MigErrorKind::InvParam, &format!("{}::safe_to_str_array: invalid string from OS", MODULE)))?));
    }

    if unsafe { SafeArrayUnaccessData(arr) } < 0 {
        return Err(report_win_api_error(MODULE, "safe_to_str_array", "SafeArrayUnaccessData"));
    }

    Err(MigError::from(MigErrorKind::NotImpl))
}

