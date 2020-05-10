use crate::{common::boot_manager::BootManager, defs::BootType};

pub(crate) mod u_boot_manager;
pub(crate) use u_boot_manager::UBootManager;
pub(crate) mod grub_boot_manager;
pub(crate) use grub_boot_manager::GrubBootManager;
pub(crate) mod raspi_boot_manager;
pub(crate) use raspi_boot_manager::RaspiBootManager;
pub(crate) mod efi_boot_manager;
pub(crate) use efi_boot_manager::EfiBootManager;

pub(crate) fn from_boot_type(boot_type: BootType) -> Box<dyn BootManager> {
    match boot_type {
        BootType::UBoot => Box::new(UBootManager::for_stage2()),
        BootType::Grub => Box::new(GrubBootManager::new()),
        BootType::Efi => Box::new(EfiBootManager::new(false)),
        BootType::MSWEfi => Box::new(EfiBootManager::new(true)),
        BootType::Raspi => Box::new(RaspiBootManager::for_stage2(boot_type)),
        BootType::Raspi64 => Box::new(RaspiBootManager::for_stage2(boot_type)),
        BootType::MSWBootMgr => panic!("BootType::MSWBootMgr is not implemented"),
    }
}
