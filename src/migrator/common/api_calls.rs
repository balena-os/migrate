use std::fs::File;
use std::io::copy;
use std::path::{Path, PathBuf};

use failure::ResultExt;
use log::debug;

use reqwest::{blocking::Client, header};
use serde::{Deserialize, Serialize};

use crate::common::{path_append, MigErrCtx, MigErrorKind};
use crate::MigError;

const OS_VERSION_URL_P1: &str = "/device-types/v1/";
const OS_VERSION_URL_P2: &str = "/images";

const OS_IMG_URL: &str = "/download";

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Versions {
    pub versions: Vec<String>,
    pub latest: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct ImageRequestData {
    #[serde(rename = "deviceType")]
    device_type: String,
    version: String,
    #[serde(rename = "fileType")]
    file_type: String,
}

pub(crate) fn get_os_versions(
    api_endpoint: &str,
    api_key: &str,
    device: &str,
) -> Result<Versions, MigError> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_str(api_key).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to create auth header"),
        ))?,
    );

    let request_url = format!(
        "{}{}{}{}",
        api_endpoint, OS_VERSION_URL_P1, device, OS_VERSION_URL_P2
    );

    debug!("get_os_versions: request_url: '{}'", request_url);

    let res = Client::builder()
        .default_headers(headers)
        .build()
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            "Failed to create https client",
        ))?
        .get(&request_url)
        .send()
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to send https request url: '{}'", request_url),
        ))?;

    debug!("Result = {:?}", res);

    let status = res.status();
    if status == 200 {
        Ok(res.json::<Versions>().context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            "Failed to parse request results",
        ))?)
    } else {
        Err(MigError::from_remark(
            MigErrorKind::InvState,
            &format!("Balena API request failed with status: {}", status),
        ))
    }
}

pub(crate) fn get_os_image(
    api_endpoint: &str,
    api_key: &str,
    device: &str,
    version: &str,
    target_dir: &Path,
) -> Result<PathBuf, MigError> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::AUTHORIZATION,
        header::HeaderValue::from_str(api_key).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to create auth header"),
        ))?,
    );

    let request_url = format!("{}{}", api_endpoint, OS_IMG_URL);

    let post_data = ImageRequestData {
        device_type: String::from(device),
        version: String::from(version),
        file_type: String::from(".gz"),
    };

    debug!("get_os_image: request_url: '{}'", request_url);
    debug!("get_os_image: data: '{:?}'", post_data);

    let mut res = Client::builder()
        .default_headers(headers)
        .build()
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            "Failed to create https client",
        ))?
        .post(&request_url)
        .json(&post_data)
        .send()
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to send https request url: '{}'", request_url),
        ))?;

    debug!("Result = {:?}", res);

    /* just results in filename "download"
    let file_name = res
        .url()
        .path_segments()
        .and_then(|segments| segments.last())
        .and_then(|name| if name.is_empty() { None } else { Some(name) })
        .unwrap_or("balen-os.img.gz");
    */

    let file_name = path_append(
        target_dir,
        &format!("balena-cloud-{}-{}.img.gz", device, version),
    );

    debug!("Downloading file '{}'", file_name.display());
    let mut file = File::create(&file_name).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("Failed to create file: '{}'", file_name.display()),
    ))?;

    copy(&mut res, &mut file).context(MigErrCtx::from_remark(
        MigErrorKind::Upstream,
        &format!("Failed to download file: '{}'", file_name.display()),
    ))?;

    Ok(file_name)
}
