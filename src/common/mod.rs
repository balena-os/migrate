//pub mod mig_error;
use crate::mig_error::MigError;

pub type OSRelease = (u32,u32,u32);

pub trait SysInfo {
    // TODO: modify to allow string references returned
    fn get_os_name(&self) -> String;
    fn get_os_release(&self) -> Option<OSRelease>;
    fn get_boot_dev(&self) -> String;
    fn get_mem_tot(&self) -> usize;
    fn get_mem_avail(&self) -> usize;
}

pub trait Migrator {
    fn can_migrate(&self) -> bool; 
    fn migrate(&self) -> Result<(),MigError>;  
}
