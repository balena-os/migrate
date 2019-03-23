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
        combaseapi::{
            CoCreateInstance, CoSetProxyBlanket,
        },
        objbase::COINIT_MULTITHREADED,
        objidl::EOAC_NONE,
        wbemcli::{CLSID_WbemLocator, IID_IWbemLocator, IWbemLocator, IWbemServices},
    },
};
use std::ptr::{self,null_mut};
use std::io::Error;
use log::{debug, warn};
use failure::{Fail};

use super::com_api::{HComApi, get_com_api};
use super::util::to_wide_cstring;
use crate::{MigError, MigErrorKind, MigErrCtx};

type PMIWbemLocator = *mut IWbemLocator;
type PMIWbemServices = *mut IWbemServices;

const MODULE: &str = "mswin::wmi_api";

pub struct WmiAPI {
    com_api: HComApi,
    p_loc: Option<PMIWbemLocator>,
    p_svc: Option<PMIWbemServices>,
} 

impl<'a> WmiAPI {
    pub fn get_api() -> Result<WmiAPI,MigError> {
        WmiAPI::get_api_from_hcom(get_com_api()?)
    }

    pub fn get_api_from_hcom(h_com_api: HComApi) -> Result<WmiAPI,MigError> {
        let mut wmi_api = Self{
            com_api: h_com_api,
            p_loc: None,
            p_svc: None,
        };

        wmi_api.create_locator()?;

        Ok(wmi_api)
        //Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn ploc(&mut self) -> Result<&'a mut PMIWbemLocator,MigError> {
        if let Some(ploc) = self.p_loc {
            Ok(ploc)
        } else {
            Err(MigError::from_remark(MigErrorKind::InvState,&format!("{}::ploc: WmiAPI is not initialized", MODULE)))
        }
    } 


    fn create_locator(&mut self) -> Result<(), MigError> {
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

        self.p_loc = Some(p_loc as PMIWbemLocator);

        debug!("Got locator {:?}", self.p_loc);

        Ok(())
    }

    fn create_services(&mut self) -> Result<(), MigError> {
        debug!("Calling ConnectServer");

        let mut p_svc = null_mut::<IWbemServices>();
        
        let mut object_path_bstr = to_wide_cstring("ROOT\\CIMV2");

        if unsafe {
            (*self.loc()).ConnectServer(
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

        self.p_svc = Some(p_svc as PMIWbemServices);

        debug!("Got service {:?}", self.p_svc);

        Ok(())
    }

} 


