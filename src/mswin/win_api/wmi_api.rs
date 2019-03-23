use failure::Fail;
use log::{debug, warn};
use std::io::Error;
use std::ptr::{self, null_mut};
use winapi::{
    shared::{
        ntdef::NULL,
        rpcdce::{
            RPC_C_AUTHN_LEVEL_CALL, RPC_C_AUTHN_LEVEL_DEFAULT, RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE,
            RPC_C_IMP_LEVEL_IMPERSONATE,
        },
        wtypesbase::CLSCTX_INPROC_SERVER,
    },
    um::{
        combaseapi::{CoCreateInstance, CoSetProxyBlanket},
        objbase::COINIT_MULTITHREADED,
        objidl::EOAC_NONE,
        wbemcli::{CLSID_WbemLocator, IID_IWbemLocator, IWbemLocator, IWbemServices},
    },
};

use super::com_api::{get_com_api, HComApi};
use super::util::to_wide_cstring;
use crate::{MigErrCtx, MigError, MigErrorKind};

type PMIWbemLocator = *mut IWbemLocator;
type PMIWbemServices = *mut IWbemServices;

const MODULE: &str = "mswin::wmi_api";

pub struct WmiAPI {
    com_api: HComApi,
    p_loc: PMIWbemLocator,
    p_svc: PMIWbemServices,
}

impl<'a> WmiAPI {
    pub fn get_api() -> Result<WmiAPI, MigError> {
        WmiAPI::get_api_from_hcom(get_com_api()?)
    }

    pub fn get_api_from_hcom(h_com_api: HComApi) -> Result<WmiAPI, MigError> {
        debug!("Calling CoCreateInstance for CLSID_WbemLocator");

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

        debug!("Got locator {:?}", p_loc);

        debug!("Calling ConnectServer");

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

        debug!("Got services {:?}", p_svc);

        let wmi_api = Self {
            com_api: h_com_api,
            p_loc: p_loc as PMIWbemLocator,
            p_svc: p_svc,
        };

        debug!("Calling CoSetProxyBlanket");

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

        Ok(wmi_api)
    }
}

/*
fn create_locator() -> Result<PMIWbemLocator, MigError> {
    debug!("Calling CoCreateInstance for CLSID_WbemLocator");

    let mut p_loc = NULL;

    if unsafe {
        CoCreateInstance(
            &CLSID_WbemLocator,
            null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_IWbemLocator,
            &mut p_loc,
        ) } < 0 {
            let os_err = Error::last_os_error();
            warn!("{}::create_locator: CoCreateInstance returned os error: {:?} ", MODULE, os_err);
            return Err(
                MigError::from(
                    os_err.context(
                        MigErrCtx::from_remark(MigErrorKind::WinApi, &format!("{}::create_locator: CoCreateInstance failed",MODULE)))));
    }

    debug!("Got locator {:?}", p_loc);

    Ok(p_loc as PMIWbemLocator)
}

fn create_services(p_loc: & PMIWbemLocator) -> Result<PMIWbemServices, MigError> {
    debug!("Calling ConnectServer");

    let mut p_svc = null_mut::<IWbemServices>();

    let mut object_path_bstr = to_wide_cstring("ROOT\\CIMV2");

    if unsafe {
        (**p_loc).ConnectServer(
            object_path_bstr.as_ptr() as *mut _,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut p_svc,
        ) } < 0 {
            let os_err = Error::last_os_error();
            warn!("{}::create_services: ConnectServer returned os error: {:?} ", MODULE, os_err);
            return Err(
                MigError::from(
                    os_err.context(
                        MigErrCtx::from_remark(MigErrorKind::WinApi, &format!("{}::create_services: ConnectServer failed",MODULE)))));
    }

    debug!("Got service {:?}", p_svc);

    Ok(p_svc as PMIWbemServices)
}
*/
