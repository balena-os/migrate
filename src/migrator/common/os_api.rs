use std::mem::transmute;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Once};

use crate::{
    common::{device_info::DeviceInfo, path_info::PathInfo, MigError},
    defs::FileType,
    defs::OSArch,
};

#[cfg(target_os = "windows")]
use crate::mswin::mswin_api::MSWinApi;

#[cfg(target_os = "linux")]
use crate::linux::{linux_api::LinuxAPI, lsblk_info::LsblkInfo};

pub(crate) trait OSApiImpl {
    fn get_os_arch(&self) -> Result<OSArch, MigError>;
    fn get_os_name(&self) -> Result<String, MigError>;

    fn path_info_from_path<P: AsRef<Path>>(&self, path: P) -> Result<PathInfo, MigError>;
    fn expect_type<P: AsRef<Path>>(&self, file: P, ftype: &FileType) -> Result<(), MigError>;
    fn canonicalize<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf, MigError>;
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
pub(crate) struct OSApi {
    inner: Arc<Mutex<OSApiContainer>>,
}

impl OSApi {
    pub fn new() -> Result<OSApi, MigError> {
        static mut OS_API: *const OSApi = 0 as *const OSApi;
        static ONCE: Once = Once::new();

        let os_api = unsafe {
            ONCE.call_once(|| {
                // Make it
                //dbg!("call_once");
                let singleton = OSApi {
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

        shared_api.api_impl = Some(OSApi::get_api()?);

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

    #[cfg(target_os = "linux")]
    pub fn get_lsblk_info(&self) -> Result<LsblkInfo, MigError> {
        Ok(self.init()?.api_impl.as_ref().unwrap().get_lsblk_info())
    }

    #[cfg(target_os = "linux")]
    pub fn device_info_from_partition<P: AsRef<Path>>(
        &self,
        part: P,
    ) -> Result<DeviceInfo, MigError> {
        let lsblk_info = self.get_lsblk_info()?;
        let (drive, partition) = lsblk_info.get_devinfo_from_partition(part.as_ref())?;
        Ok(DeviceInfo::from_lsblkinfo(drive, partition)?)
    }
}

impl OSApiImpl for OSApi {
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

    fn expect_type<P: AsRef<Path>>(&self, file: P, ftype: &FileType) -> Result<(), MigError> {
        self.init()?
            .api_impl
            .as_ref()
            .unwrap()
            .expect_type(file, ftype)
    }
}
