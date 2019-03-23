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
            CoCreateInstance, CoInitializeEx, CoInitializeSecurity, CoSetProxyBlanket,
            CoUninitialize,
        },
        objbase::COINIT_MULTITHREADED,
        objidl::EOAC_NONE,
        wbemcli::{CLSID_WbemLocator, IID_IWbemLocator, IWbemLocator, IWbemServices},
    },
};
