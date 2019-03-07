pub mod mig_error;


pub trait SysInfo {
    // TODO: modify to allow string references returned
    fn get_os_name(&self) -> String;
    fn get_os_release(&self) -> String;
    fn get_boot_dev(&self) -> String;
    fn get_mem_tot(&self) -> usize;
    fn get_mem_avail(&self) -> usize;
}
