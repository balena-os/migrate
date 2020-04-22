use std::path::PathBuf;

use log::error;

use crate::common::{
    api_calls::{get_os_image, get_os_versions},
    migrate_info::MigrateInfo,
    MigError,
};

pub(crate) fn download_image(
    mig_info: &mut MigrateInfo,
    device_type: &str,
    version: &str,
) -> Result<PathBuf, MigError> {
    if let Some(api_key) = mig_info.get_api_key() {
        let api_endpoint = mig_info.get_api_endpoint();
        let version = if version == "latest" {
            match get_os_versions(&api_endpoint, &api_key, device_type) {
                Ok(versions) => {
                    if versions.len() > 0 {
                        versions.get(0).unwrap().clone()
                    } else {
                        error!(
                            "No Balena OS Version found for device type: {}",
                            device_type
                        );
                        return Err(MigError::displayed());
                    }
                }
                Err(why) => {
                    error!(
                        "Failed to retrieve available balena versions for device type: {}, error: {:?}",
                        device_type, why
                    );
                    return Err(MigError::displayed());
                }
            }
        } else {
            String::from(version)
        };

        Ok(get_os_image(
            &api_endpoint,
            &api_key,
            device_type,
            &version,
            &mig_info.work_path.path,
        )?)
    } else {
        error!("No api-key found in config.json - unable to retrieve os-image");
        Err(MigError::displayed())
    }
}
