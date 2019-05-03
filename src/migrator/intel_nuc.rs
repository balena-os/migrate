use log::{trace, info, error};
use std::path::{Path};
use crate::stage2::{Stage2Config};

use crate::common::{
    MigError, 
    MigErrorKind,
    Stage2Info,
    Config,
    };
use crate::linux_common::{
    Device, 
    MigrateInfo,     
    is_efi_boot, 
    is_secure_boot,
    get_grub_version,
};


const MODULE: &str = "intel_nuc";

const GRUB_MIN_VERSION: &str = "2";


pub(crate) fn init_amd64(mig_info: &mut MigrateInfo) -> Result<Box<Device>, MigError> {
    trace!("{}::init_amd64: entered", MODULE);
        // **********************************************************************
    // ** AMD64 specific initialisation
    // **********************************************************************

    mig_info.efi_boot = Some(is_efi_boot()?);

    info!(
        "System is booted in {} mode",
        match mig_info.is_efi_boot() {
            true => "EFI",
            false => "Legacy BIOS",
        }
    );

    if mig_info.is_efi_boot() == true {
        // check for EFI dir & size
        mig_info.secure_boot = Some(is_secure_boot()?);
        if let Some(secure_boot) = mig_info.secure_boot {
            info!(
                "Secure boot is {}enabled",
                match secure_boot {
                    true => "",
                    false => "not ",
                }
            );
            if secure_boot == true {
                let message = format!("balena-migrate does not currently support systems with secure boot enabled.");
                error!("{}", &message);
                return Err(MigError::from_remark(MigErrorKind::InvState, &message));
            }
        }
    } else {
        mig_info.secure_boot = Some(false);
        info!("Assuming that Secure boot is not enabled for Legacy BIOS system");
    }

    let grub_version = get_grub_version()?;
    info!(
        "grub-install version is {}.{}",
        grub_version.0, grub_version.1
    );

    if grub_version.0 < String::from(GRUB_MIN_VERSION) {
        let message = format!("your version of grub-install ({}.{}) is not supported. balena-migrate requires grub version 2 or higher.", grub_version.0, grub_version.1);
        error!("{}", &message);
        return Err(MigError::from_remark(MigErrorKind::InvState, &message));
    }

    Ok(Box::new(IntelNuc{}))
}


pub(crate) struct  IntelNuc{}

impl IntelNuc {
    pub fn new() -> IntelNuc {
        IntelNuc{}
    }
}

impl<'a> Device for IntelNuc {
    fn get_device_slug(&self) -> &'static str {
        "intel-nuc"
    }

    fn setup(&self, _config: &Config, mig_info: &mut MigrateInfo) -> Result<(),MigError> {
        trace!(
            "BeagleboneGreen::setup: entered with type: '{}'",
            match &mig_info.device_slug {
                Some(s) => s,
                _ => panic!("no device type slug found"),
            }
        );

        Err(MigError::from(MigErrorKind::NotImpl))
    }
    
    fn restore_boot(&self, _root_path: &Path, _config: &Stage2Config) -> Result<(),MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }

}