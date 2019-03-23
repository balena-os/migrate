use failure::{ResultExt};
use log::{debug};
use std::ptr;
use std::slice;
use widestring::{WideCString, WideCStr};
use std::mem;

use winapi::{
    shared::{
        ntdef::NULL,
        wtypes::*,
    },    
    um::{        
        oaidl::{SAFEARRAY,VARIANT,},
        oleauto::{
            SafeArrayAccessData,
            SafeArrayUnaccessData,
            SafeArrayGetLBound,
            SafeArrayGetUBound,
            SafeArrayDestroy },
        wbemcli::{  WBEM_FLAG_ALWAYS, 
                    WBEM_FLAG_NONSYSTEM_ONLY,
                    },
    },
};

use crate::mswin::win_api::util::{report_win_api_error};
use crate::mig_error::{MigError, MigErrorKind, MigErrCtx};
use super::PMIWbemClassObject;

const MODULE: &str = "mswin::win_api::wmi_api::iwbem_class";

pub enum Variant {
    STRING(String),
    U64(u64),    
    U32(u32),
    VEC_STRING(Vec<String>),
    VEC_U32(Vec<u32>),
}

pub struct IWbemClassWrapper {
    pub inner: PMIWbemClassObject,
}

impl IWbemClassWrapper {
    pub fn new(ptr: PMIWbemClassObject) -> Self {
        Self { inner: ptr }
    }

    pub fn list_properties(&self) -> Result<Vec<String>, MigError> {
        // This will store the properties names from the GetNames call.
        let mut p_names = NULL as *mut SAFEARRAY;

        if unsafe {
            (*(self.inner)).GetNames(
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

    fn to_variant(&self, vt_prop: &VARIANT) -> Option<Variant> {
        let variant_type: VARTYPE = unsafe { vt_prop.n1.n2().vt };
        if variant_type as u32 & VT_ARRAY == VT_ARRAY {
            let array: &*mut SAFEARRAY = unsafe { vt_prop.n1.n2().n3.parray() };

            let item_type = variant_type as u32 & VT_TYPEMASK;
            if item_type ==  {

            }

            return Ok(Variant::Array(safe_array_to_vec(*array, item_type as u32)?));
        }


        None
    }

    pub fn get_property(&self, prop_name: &str) -> Result<Option<Variant>, MigError> {
        let name_prop = WideCString::from_str(prop_name).context(MigErrCtx::from(MigErrorKind::InvParam))?;

        let mut vt_prop: VARIANT = unsafe { mem::zeroed() };

        if unsafe {
            (*self.inner).Get(
                name_prop.as_ptr() as *mut _,
                0,
                &mut vt_prop,
                ptr::null_mut(),
                ptr::null_mut(),
            ) } < 0 {
                return Err(report_win_api_error(MODULE, "get_property", "IWbemClassObject::Get"));
            }

        Ok(self.to_variant(&vt_prop))        
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
        debug!("{}::safe_to_str_array: adding item: {}", MODULE, item.to_string_lossy());
        result.push(
            String::from(
                item.to_string()
                    .context(MigErrCtx::from_remark(MigErrorKind::InvParam, &format!("{}::safe_to_str_array: invalid string from OS", MODULE)))?));
    }

    if unsafe { SafeArrayUnaccessData(arr) } < 0 {
        return Err(report_win_api_error(MODULE, "safe_to_str_array", "SafeArrayUnaccessData"));
    }

    Ok(result)
}

