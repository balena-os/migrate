use failure::{ResultExt};
use log::{debug};
use std::ptr;
use std::slice;
use widestring::{WideCString, WideCStr};
use std::mem;
use std::collections::HashMap;
use std::fmt::Display;

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

#[derive(Debug)]
pub enum Variant {
    STRING(String),
    I32(i32),    
    I16(i16),
    U8(u8),
    BOOL(bool),
    NULL(),
    EMPTY(),
    UNSUPPORTED(),    
    VECSTRING(Vec<String>),
    VECI32(Vec<i32>),
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


    pub fn list_properties_wstr(&self) -> Result<Vec<WideCString>, MigError> {
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
        
        let result = safe_to_wstr_array(p_names);

        if unsafe { SafeArrayDestroy(p_names) } < 0 {
            return Err(report_win_api_error(MODULE, "list_properties", "SafeArrayDestroy"));
        }

        result
    }

    fn to_variant(&self, vt_prop: &VARIANT) -> Result<Variant, MigError> {
        let variant_type: VARTYPE = unsafe { vt_prop.n1.n2().vt };
        if variant_type as u32 & VT_ARRAY == VT_ARRAY {
            let array: &*mut SAFEARRAY = unsafe { vt_prop.n1.n2().n3.parray() };
            let item_type = variant_type as u32 & VT_TYPEMASK;
            if item_type ==  VT_BSTR {
                Ok(Variant::VECSTRING(safe_to_str_array(*array)?))
            } else if item_type ==  VT_I4 {
                Ok(Variant::VECI32(safe_to_i32_array(*array)?))
            } else {
                Err(MigError::from_remark(MigErrorKind::NotImpl,&format!("{}::to_variant: the type {} has not been implemented yet", MODULE, variant_type)))
            }
        } else {
            match variant_type as u32 {
                VT_BSTR => {
                    let bstr_ptr: &BSTR = unsafe { vt_prop.n1.n2().n3.bstrVal() };
                    let prop_val: &WideCStr = unsafe { WideCStr::from_ptr_str(*bstr_ptr) };
                    let property_value_as_string = prop_val.to_string()
                                                    .context(
                                                        MigErrCtx::from_remark(
                                                            MigErrorKind::InvParam, 
                                                            &format!("{}::to_variant: invalid character found in BSTR", MODULE)))?;
                    Ok(Variant::STRING(property_value_as_string))
                }
                VT_I2 => {
                    let num: &i16 = unsafe { vt_prop.n1.n2().n3.iVal() };
                    Ok(Variant::I16(*num))
                }
                VT_I4 => {
                    let num: &i32 = unsafe { vt_prop.n1.n2().n3.lVal() };
                    Ok(Variant::I32(*num))
                }
                VT_BOOL => {
                    let value: &i16 = unsafe { vt_prop.n1.n2().n3.boolVal() };
                    match *value {
                        VARIANT_FALSE => Ok(Variant::BOOL(false)),
                        VARIANT_TRUE => Ok(Variant::BOOL(true)),
                        _ => Err(MigError::from_remark(
                                    MigErrorKind::InvParam,
                                    &format!("{}::to_variant: an invalid bool value: {:#X} was encountered", MODULE, *value))),
                    }
                }
                VT_UI1 => {
                    let num: &i8 = unsafe { vt_prop.n1.n2().n3.cVal() };
                    Ok(Variant::U8(*num as u8))
                }
                VT_EMPTY => Ok(Variant::EMPTY()),
                VT_NULL => Ok(Variant::NULL()),
                _ => Err(MigError::from_remark(
                                    MigErrorKind::NotImpl,
                                    &format!("{}::to_variant: the variant type {} has not yet been implemented", MODULE, variant_type))),
            }
        }
    }

    pub fn get_property(&self, prop_name: &WideCString) -> Result<Option<Variant>, MigError> {
        // let name_prop = WideCString::from_str(prop_name).context(MigErrCtx::from(MigErrorKind::InvParam))?;

        let mut vt_prop: VARIANT = unsafe { mem::zeroed() };

        if unsafe {
            (*self.inner).Get(
                prop_name.as_ptr() as *mut _,
                0,
                &mut vt_prop,
                ptr::null_mut(),
                ptr::null_mut(),
            ) } < 0 {
                return Err(report_win_api_error(MODULE, "get_property", "IWbemClassObject::Get"));
            }

        // Todo find out how to detect that item is not present and return OK(None)
        Ok(Some(self.to_variant(&vt_prop)?))        
    }

    pub fn to_map(&self) -> Result<HashMap<String,Variant>,MigError> {        
        let mut result: HashMap<String,Variant> = HashMap::new();
        for prop_name_w in self.list_properties_wstr()? {
            debug!("{}::to_map: attempting property: {}", MODULE, prop_name_w.to_string_lossy());
            let prop_name = prop_name_w.to_string().context(MigErrCtx::from_remark(MigErrorKind::InvParam,&format!("{}::to_map: invalid BSTR value", MODULE)))?;
            result.entry(prop_name).or_insert(self.get_property(&prop_name_w)?.unwrap());
        }         
        Ok(result)
    }


}

impl Drop for IWbemClassWrapper {
    fn drop(&mut self) {
        debug!("{}::drop: dropping IWbemClassWrapper", MODULE);
        unsafe {
            (*self.inner).Release();
        }
    }
}


// TODO: make these generic
fn safe_to_wstr_array(arr: *mut SAFEARRAY) -> Result<Vec<WideCString>,MigError> {
    let mut p_data = NULL;
    let mut lower_bound: i32 = 0;
    let mut upper_bound: i32 = 0;

    if unsafe { SafeArrayGetLBound(arr, 1, &mut lower_bound as _) } < 0 {
        return Err(report_win_api_error(MODULE, "safe_to_wstr_array", "SafeArrayGetLBound"));
    }

    if unsafe { SafeArrayGetUBound(arr, 1, &mut upper_bound as _) } < 0 {
        return Err(report_win_api_error(MODULE, "safe_to_wstr_array", "SafeArrayGetUBound"));
    }

    if unsafe { SafeArrayAccessData(arr, &mut p_data) } < 0 {
        return Err(report_win_api_error(MODULE, "safe_to_wstr_array", "SafeArrayAccessData"));
    }

    let mut result: Vec<WideCString> = Vec::new();
    let data_slice = unsafe { slice::from_raw_parts(p_data as *mut BSTR, (upper_bound + 1) as usize) };
    let data_slice = &data_slice[(lower_bound as usize)..];
    for item_bstr in data_slice.iter() {        
        let item = unsafe { WideCString::from_ptr_str(*item_bstr) };
        debug!("{}::safe_to_wstr_array: adding item: {}", MODULE, item.to_string_lossy());
        result.push(item);
    }

    if unsafe { SafeArrayUnaccessData(arr) } < 0 {
        return Err(report_win_api_error(MODULE, "safe_to_wstr_array", "SafeArrayUnaccessData"));
    }

    Ok(result)
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
        let item = unsafe { WideCString::from_ptr_str(*item_bstr) };
        debug!("{}::safe_to_str_array: adding item: {}", MODULE, item.to_string_lossy());        
        result.push(item.to_string().context(MigErrCtx::from_remark(MigErrorKind::InvParam,&format!("{}::safe_to_str_array: invalid chars in wstring", MODULE)))?);
    }

    if unsafe { SafeArrayUnaccessData(arr) } < 0 {
        return Err(report_win_api_error(MODULE, "safe_to_str_array", "SafeArrayUnaccessData"));
    }

    Ok(result)
}

fn safe_to_i32_array(arr: *mut SAFEARRAY) -> Result<Vec<i32>,MigError> {
    let mut p_data = NULL;
    let mut lower_bound: i32 = 0;
    let mut upper_bound: i32 = 0;

    if unsafe { SafeArrayGetLBound(arr, 1, &mut lower_bound as _) } < 0 {
        return Err(report_win_api_error(MODULE, "safe_to_i32_array", "SafeArrayGetLBound"));
    }

    if unsafe { SafeArrayGetUBound(arr, 1, &mut upper_bound as _) } < 0 {
        return Err(report_win_api_error(MODULE, "safe_to_i32_array", "SafeArrayGetUBound"));
    }

    if unsafe { SafeArrayAccessData(arr, &mut p_data) } < 0 {
        return Err(report_win_api_error(MODULE, "safe_to_i32_array", "SafeArrayAccessData"));
    }

    let data_slice = unsafe { slice::from_raw_parts(p_data as *mut i32, (upper_bound + 1) as usize) };
    let data_slice = &data_slice[(lower_bound as usize)..];

    let mut result: Vec<i32> = Vec::new();

    for i32_num in data_slice.iter() {        
        debug!("{}::safe_to_i32_array: adding item: {}", MODULE, i32_num);
        result.push(*i32_num);
    }

    if unsafe { SafeArrayUnaccessData(arr) } < 0 {
        return Err(report_win_api_error(MODULE, "safe_to_i32_array", "SafeArrayUnaccessData"));
    }

    Ok(result)
}
