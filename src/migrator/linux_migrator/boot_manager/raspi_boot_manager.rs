use failure::{Fail, ResultExt};
use log::{info, trace, warn};
use regex::Regex;
use std::fs::{copy, File};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use std::time::SystemTime;

use crate::{
    common::{
        file_exists, is_balena_file, path_append,
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigErrCtx, MigError, MigErrorKind,
    },
    defs::{BootType, BALENA_FILE_TAG},
    linux_migrator::{boot_manager::BootManager, EnsuredCmds, MigrateInfo, CHMOD_CMD},
};

const RPI_MIG_KERNEL_PATH: &str = "/boot/balena.zImage";
const RPI_MIG_KERNEL_NAME: &str = "balena.zImage";

const RPI_MIG_INITRD_PATH: &str = "/boot/balena.initramfs.cpio.gz";
const RPI_MIG_INITRD_NAME: &str = "balena.initramfs.cpio.gz";

const RPI_CONFIG_TXT: &str = "config.txt";

pub(crate) struct RaspiBootManager;

impl RaspiBootManager {
    pub fn new() -> RaspiBootManager {
        RaspiBootManager {}
    }
}

impl BootManager for RaspiBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::Raspi
    }

    fn can_migrate(
        &self,
        _cmds: &mut EnsuredCmds,
        _mig_info: &MigrateInfo,
        _config: &Config,
        _s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError> {
        // TODO: calculate/ensure  required space on /boot /bootmgr
        Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn setup(
        &self,
        cmds: &EnsuredCmds,
        mig_info: &MigrateInfo,
        config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<(), MigError> {
        trace!("setup: entered with type: RaspberryPi3",);

        // **********************************************************************
        // ** copy new kernel
        let kernel_path = config.migrate.get_kernel_path();
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

        cmds.call(CHMOD_CMD, &["+x", RPI_MIG_KERNEL_PATH], false)?;

        // **********************************************************************
        // ** copy new iniramfs
        let initrd_path = config.migrate.get_initrd_path();
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

        let boot_path = &mig_info.boot_path.path;
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

            let mut boot_cfg_bckup: Vec<(String, String)> = Vec::new();
            boot_cfg_bckup.push((
                String::from(&*config_path.to_string_lossy()),
                String::from(&*backup_path.to_string_lossy()),
            ));
            s2_cfg.set_boot_bckup(boot_cfg_bckup);

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

        if !balena_config {
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

        info!(
            "Modified '{}' to boot migrate environment",
            config_path.display()
        );

        // TODO: Optional backup & modify cmd_line.txt - eg. add debug

        Ok(())
    }

    fn restore(
        &self,
        _slug: &str,
        _root_path: &Path,
        _config: &Stage2Config,
    ) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
}
