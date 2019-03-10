pub mod mig_error;
use mig_error::MigError;


pub trait SysInfo {
    // TODO: modify to allow string references returned
    fn get_os_name(&self) -> String;
    fn get_os_release(&self) -> String;
    fn get_boot_dev(&self) -> String;
    fn get_mem_tot(&self) -> usize;
    fn get_mem_avail(&self) -> usize;
}

pub trait Migrator {
    fn can_migrate(&self) -> bool; 
    fn migrate(&self) -> Result<(),MigError>;  
}
