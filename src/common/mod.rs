//pub mod mig_error;
use crate::mig_error::MigError;

pub type OSRelease = (u32,u32,u32);

pub trait SysInfo {
    // TODO: modify to allow string references returned
    fn get_os_name(&mut self) -> Result<String,MigError>;
    fn get_os_release(&mut self) -> Result<Option<OSRelease>,MigError>;
    fn get_boot_dev(&mut self) -> Result<String,MigError>;
    fn get_mem_tot(&mut self) -> Result<usize,MigError>;
    fn get_mem_avail(&mut self) -> Result<usize,MigError>;
    fn is_admin(&mut self) -> Result<bool,MigError>;
}

pub trait Migrator {
    fn can_migrate(&self) -> bool; 
    fn migrate(&self) -> Result<(),MigError>;  
}
