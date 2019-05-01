use crate::common::{
    Stage2Info, 
    MigError,
    };

pub(crate) struct Stage2Config {
    efi_boot: bool,
    drive_device: String,
    device_slug: String,
    config_path: String,
    image_path: String,
}

impl Stage2Config {
    pub fn from_config() -> Result<Stage2Config, MigError> {
        // TODO: Dummy, parse from yaml
        
        Ok(Stage2Config{
            efi_boot: false, 
            drive_device: String::from(""),
            device_slug: String::from(""),
            image_path: String::from(""),
            config_path: String::from(""),
        })
    }
}

impl<'a> Stage2Info<'a> for Stage2Config {
    fn is_efi_boot(&self) -> bool {
        self.efi_boot
    }

    fn get_drive_device(&'a self) -> &'a str {
        &self.drive_device
    }

    fn get_device_slug(&'a self) -> &'a str {
        &self.device_slug
    }

    fn get_os_image_path(&'a self) -> &'a str {
        &self.image_path
    }

    fn get_os_config_path(&'a self) -> &'a str {
        &self.config_path
    }
}