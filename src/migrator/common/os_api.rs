use std::mem::transmute;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Once};

use crate::{
    common::{
        config::migrate_config::DeviceSpec, device_info::DeviceInfo, path_info::PathInfo, MigError,
    },
    defs::{FileType, OSArch},
};

#[cfg(target_os = "windows")]
use crate::mswin::mswin_api::MSWinApi;

#[cfg(target_os = "linux")]
use crate::linux::linux_api::LinuxAPI;

pub(crate) trait OSApi {
    fn get_os_arch(&self) -> Result<OSArch, MigError>;
    fn get_os_name(&self) -> Result<String, MigError>;
    fn path_info_from_path<P: AsRef<Path>>(&self, path: P) -> Result<PathInfo, MigError>;
    fn device_path_from_partition(&self, device: &DeviceSpec) -> Result<PathBuf, MigError>;
    fn expect_type<P: AsRef<Path>>(&self, file: P, ftype: &FileType) -> Result<(), MigError>;
    fn canonicalize<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, MigError>;
    fn get_mem_info(&self) -> Result<(u64, u64), MigError>;
    fn device_info_for_efi(&self) -> Result<DeviceInfo, MigError>;
    fn to_linux_path<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, MigError>;
}

#[cfg(target_os = "windows")]
struct OSApiContainer {
    pub api_impl: Option<MSWinApi>,
}

#[cfg(target_os = "linux")]
struct OSApiContainer {
    pub api_impl: Option<LinuxAPI>,
}

#[derive(Clone)]
pub(crate) struct OSApiImpl {
    inner: Arc<Mutex<OSApiContainer>>,
}

impl OSApiImpl {
    pub fn new() -> Result<OSApiImpl, MigError> {
        static mut OS_API: *const OSApiImpl = 0 as *const OSApiImpl;
        static ONCE: Once = Once::new();

        let os_api = unsafe {
            ONCE.call_once(|| {
                // Make it
                //dbg!("call_once");
                let singleton = OSApiImpl {
                    inner: Arc::new(Mutex::new(OSApiContainer { api_impl: None })),
                };

                // Put it in the heap so it can outlive this call
                OS_API = transmute(Box::new(singleton));
            });

            (*OS_API).clone()
        };

        {
            let _dummy = os_api.init()?;
        }
        Ok(os_api)
    }

    fn init(&self) -> Result<MutexGuard<OSApiContainer>, MigError> {
        let mut shared_api = self.inner.lock().unwrap();
        if shared_api.api_impl.is_some() {
            return Ok(shared_api);
        }

        shared_api.api_impl = Some(OSApiImpl::get_api()?);

        Ok(shared_api)
    }

    #[cfg(target_os = "windows")]
    fn get_api() -> Result<MSWinApi, MigError> {
        MSWinApi::new()
    }

    #[cfg(target_os = "linux")]
    fn get_api() -> Result<LinuxAPI, MigError> {
        LinuxAPI::new()
    }
    /*
        #[cfg(target_os = "linux")]
        pub fn get_lsblk_info(&self) -> Result<LsblkInfo, MigError> {
            Ok(self.init()?.api_impl.as_ref().unwrap().get_lsblk_info())
        }
    */
}

impl OSApi for OSApiImpl {
    fn canonicalize<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, MigError> {
        self.init()?
            .api_impl
            .as_ref()
            .unwrap()
            .canonicalize(path.as_ref())
    }

    fn get_os_arch(&self) -> Result<OSArch, MigError> {
        self.init()?.api_impl.as_ref().unwrap().get_os_arch()
    }

    fn get_os_name(&self) -> Result<String, MigError> {
        self.init()?.api_impl.as_ref().unwrap().get_os_name()
    }

    fn path_info_from_path<P: AsRef<Path>>(&self, path: P) -> Result<PathInfo, MigError> {
        self.init()?
            .api_impl
            .as_ref()
            .unwrap()
            .path_info_from_path(path)
    }

    fn device_path_from_partition(&self, device: &DeviceSpec) -> Result<PathBuf, MigError> {
        self.init()?
            .api_impl
            .as_ref()
            .unwrap()
            .device_path_from_partition(device)
    }

    fn expect_type<P: AsRef<Path>>(&self, file: P, ftype: &FileType) -> Result<(), MigError> {
        self.init()?
            .api_impl
            .as_ref()
            .unwrap()
            .expect_type(file, ftype)
    }
    fn get_mem_info(&self) -> Result<(u64, u64), MigError> {
        self.init()?.api_impl.as_ref().unwrap().get_mem_info()
    }

    fn device_info_for_efi(&self) -> Result<DeviceInfo, MigError> {
        self.init()?
            .api_impl
            .as_ref()
            .unwrap()
            .device_info_for_efi()
    }

    fn to_linux_path<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, MigError> {
        Ok(self
            .init()?
            .api_impl
            .as_ref()
            .unwrap()
            .to_linux_path(path)?)
    }
}
