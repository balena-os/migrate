use failure::ResultExt;
use widestring::WideCStr;
use winapi::{
    shared::wtypes::*,
    um::oaidl::{SAFEARRAY, VARIANT},
};

use super::iwbem_class_wr::{safe_to_i32_array, safe_to_str_array};
use crate::migrator::{MigErrCtx, MigError, MigErrorKind};

const MODULE: &str = "mswin::win_api::wmi_api::variant";

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

impl Variant {
    pub fn from(vt_prop: &VARIANT) -> Result<Variant, MigError> {
        let variant_type: VARTYPE = unsafe { vt_prop.n1.n2().vt };
        if variant_type as u32 & VT_ARRAY == VT_ARRAY {
            let array: &*mut SAFEARRAY = unsafe { vt_prop.n1.n2().n3.parray() };
            let item_type = variant_type as u32 & VT_TYPEMASK;
            if item_type == VT_BSTR {
                Ok(Variant::VECSTRING(safe_to_str_array(*array)?))
            } else if item_type == VT_I4 {
                Ok(Variant::VECI32(safe_to_i32_array(*array)?))
            } else {
                Err(MigError::from_remark(
                    MigErrorKind::NotImpl,
                    &format!(
                        "{}::to_variant: the type {} has not been implemented yet",
                        MODULE, variant_type
                    ),
                ))
            }
        } else {
            match variant_type as u32 {
                VT_BSTR => {
                    let bstr_ptr: &BSTR = unsafe { vt_prop.n1.n2().n3.bstrVal() };
                    let prop_val: &WideCStr = unsafe { WideCStr::from_ptr_str(*bstr_ptr) };
                    let property_value_as_string =
                        prop_val.to_string().context(MigErrCtx::from_remark(
                            MigErrorKind::InvParam,
                            &format!("{}::to_variant: invalid character found in BSTR", MODULE),
                        ))?;
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
                            &format!(
                                "{}::to_variant: an invalid bool value: {:#X} was encountered",
                                MODULE, *value
                            ),
                        )),
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
                    &format!(
                        "{}::to_variant: the variant type {} has not yet been implemented",
                        MODULE, variant_type
                    ),
                )),
            }
        }
    }
}
