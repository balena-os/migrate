use super::OSArch;
// Contains all relevant information gathered during phase 1
// implemented as trait to allow individual implementation on windows

pub trait Stage1Info<'a> {
    fn get_os_name(&'a self) -> &'a str;
    fn get_drive_size(&self) -> u64;
    fn get_boot_path(&'a self) -> &'a str;
    fn get_boot_device(&'a self) -> &'a str;
    fn get_root_path(&'a self) -> &'a str;
    fn get_root_device(&'a self) -> &'a str;
    fn get_efi_device(&'a self) -> Option<&'a str>;
    fn get_device_slug(&'a self) -> &'a str;
}

pub trait Stage2Info<'a> {
    fn is_efi_boot(&self) -> bool;
    fn get_drive_device(&'a self) -> &'a str;   
    fn get_work_path(&'a self) -> &'a str;
    fn get_os_arch(&'a self) -> &'a OSArch;
}