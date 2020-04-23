use std::path::PathBuf;

use log::{error, info, warn};

use semver::{Identifier, Version, VersionReq};

use crate::common::{
    api_calls::{get_os_image, get_os_versions},
    migrate_info::MigrateInfo,
    MigErrCtx, MigError, MigErrorKind,
};
use failure::ResultExt;

const SUPPORTED_DEVICES: [&str; 1] = ["raspberrypi3"];

pub(crate) fn download_image(
    mig_info: &mut MigrateInfo,
    device_type: &str,
    version: &str,
) -> Result<PathBuf, MigError> {
    if !SUPPORTED_DEVICES.contains(&device_type) {
        error!(
            "OS download is not supported for devie type {}",
            device_type
        );
        return Err(MigError::displayed());
    }

    if let Some(api_key) = mig_info.get_api_key() {
        let api_endpoint = mig_info.get_api_endpoint();

        let mut versions = get_os_versions(&api_endpoint, &api_key, device_type)?;
        versions.versions.sort();
        versions.versions.reverse();

        let version = match version {
            "latest" => Version::parse(&versions.latest).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to parse version from '{}'", versions.latest),
            ))?,
            "default" => {
                let mut found: Option<Version> = None;
                for ref ver_str in versions.versions {
                    match Version::parse(ver_str) {
                        Ok(cmp_ver) => {
                            if cmp_ver.is_prerelease() {
                                continue;
                            } else {
                                if cmp_ver
                                    .build
                                    .contains(&Identifier::AlphaNumeric("prod".to_string()))
                                {
                                    found = Some(cmp_ver);
                                    break;
                                }
                            }
                        }
                        Err(why) => {
                            warn!(
                                "Failed to parse version from '{}', error: {:?}",
                                ver_str, why
                            );
                            continue;
                        }
                    }
                }
                if let Some(found) = found {
                    found
                } else {
                    error!("No version found for '{}'", version);
                    return Err(MigError::displayed());
                }
            }
            _ => {
                if version.starts_with("^") || version.starts_with("~") {
                    let ver_req = VersionReq::parse(version).context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("Failed to parse version from '{}'", version),
                    ))?;
                    let mut found: Option<Version> = None;
                    for ver_str in &versions.versions {
                        match Version::parse(ver_str) {
                            Ok(cmp_ver) => {
                                if ver_req.matches(&cmp_ver)
                                    && !cmp_ver.is_prerelease()
                                    && cmp_ver
                                        .build
                                        .contains(&Identifier::AlphaNumeric("prod".to_string()))
                                {
                                    found = Some(cmp_ver);
                                    break;
                                }
                            }
                            Err(why) => {
                                warn!(
                                    "Failed to parse version from '{}', error: {:?}",
                                    ver_str, why
                                );
                                continue;
                            }
                        }
                    }
                    if let Some(found) = found {
                        found
                    } else {
                        error!("No version found for '{}'", version);
                        return Err(MigError::displayed());
                    }
                } else {
                    let ver_req = Version::parse(version).context(MigErrCtx::from_remark(
                        MigErrorKind::Upstream,
                        &format!("Failed to parse version from '{}'", version),
                    ))?;

                    let mut found: Option<Version> = None;
                    for ver_str in &versions.versions {
                        match Version::parse(ver_str) {
                            Ok(cmp_ver) => {
                                if ver_req == cmp_ver
                                    && !cmp_ver.is_prerelease()
                                    && (cmp_ver.build == ver_req.build
                                        || cmp_ver.build.contains(&Identifier::AlphaNumeric(
                                            "prod".to_string(),
                                        )))
                                {
                                    found = Some(cmp_ver);
                                    break;
                                }
                            }
                            Err(why) => {
                                warn!(
                                    "Failed to parse version from '{}', error: {:?}",
                                    ver_str, why
                                );
                                continue;
                            }
                        }
                    }
                    if let Some(found) = found {
                        found
                    } else {
                        error!("No version found for '{}'", version);
                        return Err(MigError::displayed());
                    }
                }
            }
        };

        info!(
            "Downloading Balena OS image, selected version is: '{}'",
            version.to_string()
        );

        // TODO: extract OS image for flasher

        Ok(get_os_image(
            &api_endpoint,
            &api_key,
            device_type,
            &version.to_string(),
            &mig_info.work_path.path,
        )?)
    } else {
        error!("No api-key found in config.json - unable to retrieve os-image");
        Err(MigError::displayed())
    }
}
