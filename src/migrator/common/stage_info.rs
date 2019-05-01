use super::OSArch;
// Contains all relevant information gathered during phase 1
// implemented as trait to allow individual implementation on windows

pub const EFI_BOOT_KEY: &str = "efi_boot";
//pub const DRIVE_DEVICE_KEY: &str = "drive_device";
pub const ROOT_DEVICE_KEY: &str = "root_device";
pub const BOOT_DEVICE_KEY: &str = "boot_device";
pub const DEVICE_SLUG_KEY: &str = "device_slug";
pub const BALENA_IMAGE_KEY: &str = "balena_image";
pub const BALENA_CONFIG_KEY: &str = "balena_config";
pub const BACKUP_CONFIG_KEY: &str = "backup_config";
pub const BACKUP_ORIG_KEY: &str = "orig";
pub const BACKUP_BCKUP_KEY: &str = "bckup";


pub trait Stage1Info<'a> {
    fn get_os_name(&'a self) -> &'a str;
    fn get_drive_size(&self) -> u64;
    fn get_efi_device(&'a self) -> Option<&'a str>;    
    fn get_work_path(&'a self) -> &'a str;
    fn get_os_arch(&'a self) -> &'a OSArch;
    fn get_drive_device(&'a self) -> &'a str;           
}

pub trait Stage2Info<'a> {
    fn is_efi_boot(&self) -> bool;
    fn get_device_slug(&'a self) -> &'a str;
    fn get_balena_image(&'a self) -> &'a str;
    fn get_balena_config(&'a self) -> &'a str;
    fn get_backups(&'a self) -> &'a Vec<(String,String)>;
    fn get_root_device(&'a self) -> &'a str;
    fn get_boot_device(&'a self) -> &'a str;
}