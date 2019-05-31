use failure::ResultExt;
use log::{debug, trace};
use std::collections::HashMap;
use std::mem;
use std::ptr;
use std::slice;
use widestring::WideCString;

use winapi::{
    shared::{ntdef::NULL, wtypes::*},
    um::{
        oaidl::{SAFEARRAY, VARIANT},
        oleauto::{
            SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetLBound, SafeArrayGetUBound,
            SafeArrayUnaccessData,
        },
        wbemcli::{WBEM_FLAG_ALWAYS, WBEM_FLAG_NONSYSTEM_ONLY},
    },
};

use crate::{
    mswin::win_api::util::report_win_api_error, 
    common::{MigErrCtx, MigError, MigErrorKind,},
};

use super::variant::Variant;
use super::PMIWbemClassObject;

const MODULE: &str = "mswin::win_api::wmi_api::iwbem_class_wr";

pub struct IWbemClassWrapper {
    pub inner: PMIWbemClassObject,
}

impl IWbemClassWrapper {
    pub fn new(ptr: PMIWbemClassObject) -> Self {
        debug!(
            "{}::new: creating IWbemClassWrapper, ptr: {:?}",
            MODULE, ptr
        );
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
            )
        } < 0
        {
            return Err(report_win_api_error(
                MODULE,
                "list_properties",
                "IWbemClassObject::GetNames",
            ));
        }

        let result = safe_to_str_array(p_names);

        if unsafe { SafeArrayDestroy(p_names) } < 0 {
            return Err(report_win_api_error(
                MODULE,
                "list_properties",
                "SafeArrayDestroy",
            ));
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
            )
        } < 0
        {
            return Err(report_win_api_error(
                MODULE,
                "list_properties",
                "IWbemClassObject::GetNames",
            ));
        }

        let result = safe_to_wstr_array(p_names);

        if unsafe { SafeArrayDestroy(p_names) } < 0 {
            return Err(report_win_api_error(
                MODULE,
                "list_properties",
                "SafeArrayDestroy",
            ));
        }

        result
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
            )
        } < 0
        {
            return Err(report_win_api_error(
                MODULE,
                "get_property",
                "IWbemClassObject::Get",
            ));
        }

        // Todo find out how to detect that item is not present and return OK(None)
        Ok(Some(Variant::from(&vt_prop)?))
    }

    pub fn to_map(&self) -> Result<HashMap<String, Variant>, MigError> {
        let mut result: HashMap<String, Variant> = HashMap::new();
        for prop_name_w in self.list_properties_wstr()? {
            trace!(
                "{}::to_map: attempting property: {}",
                MODULE,
                prop_name_w.to_string_lossy()
            );
            let prop_name = prop_name_w.to_string().context(MigErrCtx::from_remark(
                MigErrorKind::InvParam,
                &format!("{}::to_map: invalid BSTR value", MODULE),
            ))?;
            result
                .entry(prop_name)
                .or_insert(self.get_property(&prop_name_w)?.unwrap());
        }
        Ok(result)
    }
}

impl Drop for IWbemClassWrapper {
    fn drop(&mut self) {
        debug!(
            "{}::drop: dropping IWbemClassWrapper, ptr: {:?}",
            MODULE, self.inner
        );
        unsafe {
            (*self.inner).Release();
        }
    }
}

// TODO: make these generic
pub(crate) fn safe_to_wstr_array(arr: *mut SAFEARRAY) -> Result<Vec<WideCString>, MigError> {
    let mut p_data = NULL;
    let mut lower_bound: i32 = 0;
    let mut upper_bound: i32 = 0;

    if unsafe { SafeArrayGetLBound(arr, 1, &mut lower_bound as _) } < 0 {
        return Err(report_win_api_error(
            MODULE,
            "safe_to_wstr_array",
            "SafeArrayGetLBound",
        ));
    }

    if unsafe { SafeArrayGetUBound(arr, 1, &mut upper_bound as _) } < 0 {
        return Err(report_win_api_error(
            MODULE,
            "safe_to_wstr_array",
            "SafeArrayGetUBound",
        ));
    }

    if unsafe { SafeArrayAccessData(arr, &mut p_data) } < 0 {
        return Err(report_win_api_error(
            MODULE,
            "safe_to_wstr_array",
            "SafeArrayAccessData",
        ));
    }

    let mut result: Vec<WideCString> = Vec::new();
    let data_slice =
        unsafe { slice::from_raw_parts(p_data as *mut BSTR, (upper_bound + 1) as usize) };
    let data_slice = &data_slice[(lower_bound as usize)..];
    for item_bstr in data_slice.iter() {
        let item = unsafe { WideCString::from_ptr_str(*item_bstr) };
        trace!(
            "{}::safe_to_wstr_array: adding item: {}",
            MODULE,
            item.to_string_lossy()
        );
        result.push(item);
    }

    if unsafe { SafeArrayUnaccessData(arr) } < 0 {
        return Err(report_win_api_error(
            MODULE,
            "safe_to_wstr_array",
            "SafeArrayUnaccessData",
        ));
    }

    Ok(result)
}

pub(crate) fn safe_to_str_array(arr: *mut SAFEARRAY) -> Result<Vec<String>, MigError> {
    let mut p_data = NULL;
    let mut lower_bound: i32 = 0;
    let mut upper_bound: i32 = 0;

    if unsafe { SafeArrayGetLBound(arr, 1, &mut lower_bound as _) } < 0 {
        return Err(report_win_api_error(
            MODULE,
            "safe_to_str_array",
            "SafeArrayGetLBound",
        ));
    }

    if unsafe { SafeArrayGetUBound(arr, 1, &mut upper_bound as _) } < 0 {
        return Err(report_win_api_error(
            MODULE,
            "safe_to_str_array",
            "SafeArrayGetUBound",
        ));
    }

    if unsafe { SafeArrayAccessData(arr, &mut p_data) } < 0 {
        return Err(report_win_api_error(
            MODULE,
            "safe_to_str_array",
            "SafeArrayAccessData",
        ));
    }

    let mut result: Vec<String> = Vec::new();
    let data_slice =
        unsafe { slice::from_raw_parts(p_data as *mut BSTR, (upper_bound + 1) as usize) };
    let data_slice = &data_slice[(lower_bound as usize)..];
    for item_bstr in data_slice.iter() {
        let item = unsafe { WideCString::from_ptr_str(*item_bstr) };
        trace!(
            "{}::safe_to_str_array: adding item: {}",
            MODULE,
            item.to_string_lossy()
        );
        result.push(item.to_string().context(MigErrCtx::from_remark(
            MigErrorKind::InvParam,
            &format!("{}::safe_to_str_array: invalid chars in wstring", MODULE),
        ))?);
    }

    if unsafe { SafeArrayUnaccessData(arr) } < 0 {
        return Err(report_win_api_error(
            MODULE,
            "safe_to_str_array",
            "SafeArrayUnaccessData",
        ));
    }

    Ok(result)
}

pub(crate) fn safe_to_i32_array(arr: *mut SAFEARRAY) -> Result<Vec<i32>, MigError> {
    let mut p_data = NULL;
    let mut lower_bound: i32 = 0;
    let mut upper_bound: i32 = 0;

    if unsafe { SafeArrayGetLBound(arr, 1, &mut lower_bound as _) } < 0 {
        return Err(report_win_api_error(
            MODULE,
            "safe_to_i32_array",
            "SafeArrayGetLBound",
        ));
    }

    if unsafe { SafeArrayGetUBound(arr, 1, &mut upper_bound as _) } < 0 {
        return Err(report_win_api_error(
            MODULE,
            "safe_to_i32_array",
            "SafeArrayGetUBound",
        ));
    }

    if unsafe { SafeArrayAccessData(arr, &mut p_data) } < 0 {
        return Err(report_win_api_error(
            MODULE,
            "safe_to_i32_array",
            "SafeArrayAccessData",
        ));
    }

    let data_slice =
        unsafe { slice::from_raw_parts(p_data as *mut i32, (upper_bound + 1) as usize) };
    let data_slice = &data_slice[(lower_bound as usize)..];

    let mut result: Vec<i32> = Vec::new();

    for i32_num in data_slice.iter() {
        trace!("{}::safe_to_i32_array: adding item: {}", MODULE, i32_num);
        result.push(*i32_num);
    }

    if unsafe { SafeArrayUnaccessData(arr) } < 0 {
        return Err(report_win_api_error(
            MODULE,
            "safe_to_i32_array",
            "SafeArrayUnaccessData",
        ));
    }

    Ok(result)
}
