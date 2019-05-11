use failure::{Fail, ResultExt};
use log::{error, info, trace, warn};
use regex::{Regex};
use std::fs::{copy, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::SystemTime;

use crate::{
    common::{file_exists, is_balena_file, path_append, Config, MigErrCtx, MigError, MigErrorKind},
    defs::BALENA_FILE_TAG,
    linux_common::{call_cmd, migrate_info::MigrateInfo, CHMOD_CMD},
    stage2::Stage2Config,
    device::Device,
};

const RPI_MODEL_REGEX: &str = r#"^Raspberry\s+Pi\s+(\S+)\s+Model\s+(.*)$"#;
const RPI_CONFIG_TXT: &str = "config.txt";

const RPI_MIG_KERNEL_PATH: &str = "/boot/balena.zImage";
const RPI_MIG_KERNEL_NAME: &str = "balena.zImage";

const RPI_MIG_INITRD_PATH: &str = "/boot/balena.initramfs.cpio.gz";
const RPI_MIG_INITRD_NAME: &str = "balena.initramfs.cpio.gz";

pub(crate) fn is_rpi(model_string: &str) -> Result<Box<Device>, MigError> {
    trace!(
        "Beaglebone::is_bb: entered with model string: '{}'",
        model_string
    );

    if let Some(captures) = Regex::new(RPI_MODEL_REGEX).unwrap().captures(model_string) {
        let pitype = captures.get(1).unwrap().as_str();
        let model = captures
            .get(2)
            .unwrap()
            .as_str()
            .trim_matches(char::from(0));

        match pitype {
            "3" => {
                info!("Identified RaspberryPi3: model {}", model);
                Ok(Box::new(RaspberryPi3 {}))
            }
            _ => {
                let message = format!("The raspberry pi type reported by your device ('{} {}') is not supported by balena-migrate", pitype, model);
                error!("{}", message);
                Err(MigError::from_remark(MigErrorKind::InvParam, &message))
            }
        }
    } else {
        warn!("no match for Raspberry PI on: {}", model_string);
        Err(MigError::from(MigErrorKind::NoMatch))
    }
}

pub(crate) struct RaspberryPi3 {}

impl RaspberryPi3 {
    pub(crate) fn new() -> RaspberryPi3 {
        RaspberryPi3 {}
    }
}

impl<'a> Device for RaspberryPi3 {
    fn get_device_slug(&self) -> &'static str {
        "raspberrypi3"
    }

    fn setup(&self, _config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
        trace!(
            "RaspberryPi3::setup: entered with type: '{}'",
            match &mig_info.device_slug {
                Some(s) => s,
                _ => panic!("no device type slug found"),
            }
        );

        // **********************************************************************
        // ** copy new kernel
        let kernel_path = mig_info.get_kernel_path();
        std::fs::copy(kernel_path, RPI_MIG_KERNEL_PATH).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to copy kernel file '{}' to '{}'",
                kernel_path.display(),
                RPI_MIG_KERNEL_PATH
            ),
        ))?;
        info!(
            "copied kernel: '{}' -> '{}'",
            kernel_path.display(),
            RPI_MIG_KERNEL_PATH
        );
        call_cmd(CHMOD_CMD, &["+x", RPI_MIG_KERNEL_PATH], false)?;

        // **********************************************************************
        // ** copy new iniramfs
        let initrd_path = mig_info.get_initrd_path();
        std::fs::copy(initrd_path, RPI_MIG_INITRD_PATH).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to copy initrd file '{}' to '{}'",
                initrd_path.display(),
                RPI_MIG_INITRD_PATH
            ),
        ))?;
        info!(
            "copied initramfs: '{}' -> '{}'",
            initrd_path.display(),
            RPI_MIG_INITRD_PATH
        );

        let boot_path = mig_info.get_boot_path();
        let config_path = path_append(boot_path, RPI_CONFIG_TXT);

        if !file_exists(&config_path) {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("Could not find '{}'", config_path.display()),
            ));
        }

        // create backup of config.txt

        let balena_config = is_balena_file(&config_path)?;
        if !balena_config {
            let system_time = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    "Failed to create timestamp",
                ))?;
            let backup_path = path_append(
                boot_path,
                &format!("{}.{}", RPI_CONFIG_TXT, system_time.as_secs()),
            );

            copy(&config_path, &backup_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to copy '{}' to '{}'",
                    config_path.display(),
                    backup_path.display()
                ),
            ))?;
            mig_info.boot_cfg_bckup.push((
                String::from(&*config_path.to_string_lossy()),
                String::from(&*backup_path.to_string_lossy()),
            ));
            info!(
                "Created backup of '{}' in '{}'",
                config_path.display(),
                backup_path.display()
            );
        } else {
            // TODO: what to do if it is a balena-migrate created config.txt ?
            warn!("We appear to be modifying a '{}' that has been created by balena-migrate. No original config backup will be available as fallback.", &config_path.display());
        }

        let initrd_re = Regex::new(r#"^\s*initramfs"#).unwrap();
        let mut initrd_found = false;

        let kernel_re = Regex::new(r#"^\s*kernel"#).unwrap();
        let mut kernel_found = false;

        let mut out_str = String::new();

        if ! balena_config {
            out_str += &format!("{}\n", BALENA_FILE_TAG);
        }

        {
            let config_file = File::open(&config_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to open file '{}'", config_path.display()),
            ))?;
            for line in BufReader::new(config_file).lines() {
                match line {
                    Ok(line) => {
                        // TODO: more modifications to /boot/config.txt
                        if initrd_re.is_match(&line) {
                            // save commented version anyway
                            out_str.push_str(&format!("# {}\n", line));
                            if !initrd_found {
                                out_str.push_str(&format!(
                                    "initramfs {} followkernel\n",
                                    RPI_MIG_INITRD_NAME
                                ));
                                initrd_found = true;
                            }
                        } else if kernel_re.is_match(&line) {
                            // save commented version anyway
                            out_str.push_str(&format!("# {}\n", line));
                            if !kernel_found {
                                out_str.push_str(&format!("kernel {}\n", RPI_MIG_KERNEL_NAME));
                                kernel_found = true;
                            }
                        } else {
                            out_str.push_str(&format!("{}\n", &line));
                        }
                    }
                    Err(why) => {
                        return Err(MigError::from(why.context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!("Failed to read line from file '{}'", config_path.display()),
                        ))));
                    }
                }
            }
        }

        if !initrd_found {
            // add it if it did not exist
            out_str.push_str(&format!("initramfs {} followkernel\n", RPI_MIG_INITRD_NAME));
        }

        if !kernel_found {
            // add it if it did not exist
            out_str.push_str(&format!("kernel {}\n", RPI_MIG_KERNEL_NAME));
        }

        let mut config_file = File::create(&config_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Failed to open file '{}' for writing",
                config_path.display()
            ),
        ))?;

        config_file
            .write(out_str.as_bytes())
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed write to file '{}'", config_path.display()),
            ))?;

        info!("Modified '{}' to boot migrate environment", config_path.display());

        // TODO: Optional backup & modify cmd_line.txt - eg. add debug

        Ok(())
    }

    fn can_migrate(&self, _config: &Config, _mig_info: &mut MigrateInfo) -> Result<bool, MigError> {
        // TODO: check
        Ok(true)
    }

    fn restore_boot(&self, _root_path: &Path, _config: &Stage2Config) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
}
