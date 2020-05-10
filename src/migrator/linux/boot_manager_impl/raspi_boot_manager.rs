use failure::{Fail, ResultExt};
use log::{debug, error, info, trace, warn};
use regex::Regex;
use std::fs::{copy, read_dir, read_to_string, File};
use std::io::{BufRead, BufReader, Write};

use std::time::SystemTime;

use crate::common::format_size_with_unit;
use crate::common::stage2_config::BackupCfg;
use crate::defs::{MIG_INITRD_NAME, MIG_KERNEL_NAME};
use crate::{
    common::{
        boot_manager::BootManager,
        call, dir_exists, file_exists, is_balena_file,
        migrate_info::MigrateInfo,
        path_append,
        path_info::PathInfo,
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigErrCtx, MigError, MigErrorKind,
    },
    defs::{BootType, BALENA_FILE_TAG},
    linux::{linux_defs::BOOT_PATH, linux_defs::CHMOD_CMD, stage2::mounts::Mounts},
};
use std::path::PathBuf;

// TODO: copy rpi dtb's , backup orig dtbs

const RPI_CONFIG_TXT: &str = "config.txt";
const RPI_CMDLINE_TXT: &str = "cmdline.txt";
const RPI_BOOT_PATH: &str = "/boot";

// TODO: more specific lists for PRI types ?

pub(crate) struct RaspiBootManager {
    bootmgr_path: Option<PathInfo>,
    boot_type: BootType,
}

#[allow(clippy::new_ret_no_self)] //TODO refactor this to fix cluppy warning
impl RaspiBootManager {
    pub fn for_stage2(boot_type: BootType) -> impl BootManager + 'static {
        RaspiBootManager {
            bootmgr_path: None,
            boot_type,
        }
    }

    pub fn new(boot_type: BootType) -> Result<impl BootManager + 'static, MigError> {
        match boot_type {
            BootType::Raspi => Ok(RaspiBootManager {
                bootmgr_path: None,
                boot_type,
            }),
            BootType::Raspi64 => Ok(RaspiBootManager {
                bootmgr_path: None,
                boot_type,
            }),
            _ => {
                error!(
                    "Invalid boot type encountered for RaspiBootManager: {:?}",
                    boot_type
                );
                Err(MigError::displayed())
            }
        }
    }
}

impl BootManager for RaspiBootManager {
    fn get_boot_type(&self) -> BootType {
        self.boot_type
    }

    fn get_bootmgr_path(&self) -> PathInfo {
        self.bootmgr_path.as_ref().unwrap().clone()
    }

    fn can_migrate(
        &mut self,
        mig_info: &MigrateInfo,
        _config: &Config,
        _s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError> {
        // TODO: calculate/ensure  required space on /boot /bootmgr

        if !dir_exists(RPI_BOOT_PATH)? {
            error!("The /boot directory required for the raspi boot manager could not be found");
            return Ok(false);
        }

        let bootmgr_path = PathInfo::from_path(RPI_BOOT_PATH)?;
        let asset_ver = mig_info.assets.get_version()?;
        let mut boot_req_space = asset_ver.asset_size + 8 * 1024;

        let kernel_path = path_append(RPI_BOOT_PATH, MIG_KERNEL_NAME);
        if kernel_path.exists() {
            boot_req_space -= kernel_path
                .metadata()
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed to retrieve metadata for file '{}'",
                        kernel_path.display()
                    ),
                ))?
                .len()
        }

        let initrd_path = path_append(RPI_BOOT_PATH, MIG_INITRD_NAME);
        if initrd_path.exists() {
            boot_req_space -= initrd_path
                .metadata()
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed to retrieve metadata for file '{}'",
                        initrd_path.display()
                    ),
                ))?
                .len()
        }

        if boot_req_space < bootmgr_path.fs_free {
            self.bootmgr_path = Some(bootmgr_path);
            Ok(true)
        } else {
            error!(
                "Insufficient space found in boot directory: required {}, found {}",
                format_size_with_unit(boot_req_space),
                format_size_with_unit(bootmgr_path.fs_free)
            );
            // enough space to install hereget_
            Ok(false)
        }
    }

    #[allow(clippy::cognitive_complexity)] //TODO refactor this function to fix the clippy warning
    fn setup(
        &mut self,
        mig_info: &MigrateInfo,
        _config: &Config,
        s2_cfg: &mut Stage2ConfigBuilder,
        kernel_opts: &str,
    ) -> Result<(), MigError> {
        debug!("setup: entered with type: {:?}", self.boot_type);

        let mut boot_cfg_bckup: Vec<BackupCfg> = Vec::new();

        let system_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                "Failed to create timestamp",
            ))?;

        // backup dtb files
        let dir_contents = read_dir(RPI_BOOT_PATH).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("Failed to read contents of '{}'", RPI_BOOT_PATH),
        ))?;

        // create backup of dtb files
        for dir_entry in dir_contents {
            if let Ok(dir_entry) = dir_entry {
                if let Some(extension) = dir_entry.path().extension() {
                    if &*extension.to_string_lossy().to_lowercase() == "dtb" {
                        let bckup_path = PathBuf::from(&format!(
                            "{}-{}",
                            &*dir_entry.path().to_string_lossy(),
                            system_time.as_secs()
                        ));
                        debug!(
                            "Creating backup of '{}' in '{}'",
                            dir_entry.path().display(),
                            bckup_path.display()
                        );
                        copy(&dir_entry.path(), &bckup_path).context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!(
                                "Failed to copy '{}' to '{}'",
                                dir_entry.path().display(),
                                bckup_path.display()
                            ),
                        ))?;
                        boot_cfg_bckup.push(BackupCfg::new(&dir_entry.path(), &bckup_path)?);
                    }
                }
            }
        }
        // **********************************************************************
        // ** copy new kernel

        info!(
            "Writing migrate kernel & initramfs & dtb's to '{}'",
            BOOT_PATH
        );
        mig_info.assets.write_to(RPI_BOOT_PATH)?;

        let cmd_res = call(
            CHMOD_CMD,
            &[
                "+x",
                &*path_append(RPI_BOOT_PATH, MIG_KERNEL_NAME).to_string_lossy(),
            ],
            false,
        )?;

        if !cmd_res.status.success() {
            error!(
                "Failed to set new kernel executable flag, error: {}",
                cmd_res.stderr
            );
            return Err(MigError::displayed());
        }

        let boot_path = if let Some(ref boot_path) = self.bootmgr_path {
            boot_path
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                "bootmgr_path is not configured",
            ));
        };

        let config_path = path_append(&boot_path.path, RPI_CONFIG_TXT);

        if !file_exists(&config_path) {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("Could not find '{}'", config_path.display()),
            ));
        }

        let balena_config = is_balena_file(&config_path)?;
        // TODO: try to make sure valid configs, are saved independently from migrator tag
        if !balena_config {
            // backup config.txt
            let backup_file = format!("{}-{}", RPI_CONFIG_TXT, system_time.as_secs());
            let backup_path = path_append(&boot_path.path, &backup_file);

            copy(&config_path, &backup_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to copy '{}' to '{}'",
                    config_path.display(),
                    backup_path.display()
                ),
            ))?;

            boot_cfg_bckup.push(BackupCfg::new(&config_path, &backup_path)?);

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
        let kernel_re = Regex::new(r#"^\s*kernel"#).unwrap();
        let uart_re = Regex::new(r#"^\s*enable_uart"#).unwrap();
        let sixty4_bits_re = Regex::new(r#"^\s*arm_64bit"#).unwrap();

        let mut config_str = String::new();

        if !balena_config {
            config_str += &format!("{}\n", BALENA_FILE_TAG);
        }

        {
            let config_file = File::open(&config_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to open file '{}'", config_path.display()),
            ))?;

            // TODO: remove potentially harmfull configs

            for line in BufReader::new(config_file).lines() {
                match line {
                    Ok(line) => {
                        // TODO: more modifications to /boot/config.txt
                        if initrd_re.is_match(&line)
                            || kernel_re.is_match(&line)
                            || uart_re.is_match(&line)
                        {
                            config_str.push_str(&format!("# {}\n", line));
                        } else if let BootType::Raspi64 = self.boot_type {
                            if sixty4_bits_re.is_match(&line) {
                                config_str.push_str(&format!("# {}\n", line));
                            } else {
                                config_str.push_str(&format!("{}\n", &line));
                            }
                        } else {
                            config_str.push_str(&format!("{}\n", &line));
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

        if let BootType::Raspi64 = self.boot_type {
            config_str.push_str("arm_64bit=1\n");
        }

        config_str.push_str("enable_uart=1\n");
        config_str.push_str(&format!("initramfs {} followkernel\n", MIG_INITRD_NAME));
        config_str.push_str(&format!("kernel {}\n", MIG_KERNEL_NAME));

        info!(
            "Modified '{}' to boot migrate environment",
            config_path.display()
        );

        let cmdline_path = path_append(&boot_path.path, RPI_CMDLINE_TXT);
        // Assume we have to backup cmdline.txt if we had to backup config.txt
        if !balena_config {
            // backup cmdline.txt
            let backup_file = format!("{}-{}", RPI_CMDLINE_TXT, system_time.as_secs());
            let backup_path = path_append(&boot_path.path, &backup_file);

            copy(&cmdline_path, &backup_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to copy '{}' to '{}'",
                    cmdline_path.display(),
                    backup_path.display()
                ),
            ))?;

            boot_cfg_bckup.push(BackupCfg::new(&cmdline_path, &backup_path)?);
        }

        let cmdline_str = match read_to_string(&cmdline_path) {
            Ok(cmdline) => {
                let cmdline = cmdline.trim_end();

                trace!("cmdline: '{}'", cmdline);
                // Add or replace root command to cmdline
                let cmd_fragment = format!(" root={} ", &boot_path.device_info.get_kernel_cmd());
                let cmd_len = cmd_fragment.len();

                let mut mod_cmdline = String::from(
                    Regex::new(r#"root=\S+(\s+|$)"#)
                        .unwrap()
                        .replace(cmdline, &cmd_fragment[1..]),
                );

                if !mod_cmdline.contains(&cmd_fragment[1..cmd_len - 1]) {
                    mod_cmdline.push_str(&cmd_fragment[..cmd_len - 1]);
                }

                trace!("cmdline: '{}'", mod_cmdline);

                // Add root fs type to cmdline
                let cmd_fragment = format!(" rootfstype={} ", &boot_path.device_info.fs_type);
                let cmd_len = cmd_fragment.len();
                mod_cmdline = String::from(
                    Regex::new(r#"rootfstype=\S+(\s+|$)"#)
                        .unwrap()
                        .replace(mod_cmdline.as_ref(), &cmd_fragment[1..]),
                );
                if !mod_cmdline.contains(&cmd_fragment[1..cmd_len - 1]) {
                    mod_cmdline.push_str(&cmd_fragment[..cmd_len - 1]);
                }

                trace!("cmdline: '{}'", mod_cmdline);
                // make sure console points to the right thing
                // TODO: make configurable
                let rep = " ";
                mod_cmdline = String::from(
                    Regex::new(r#"console=\S+((\s+|$))"#)
                        .unwrap()
                        .replace_all(mod_cmdline.as_ref(), rep),
                );
                mod_cmdline.push_str(&" console=tty1 console=serial0,115200".to_string());

                trace!("cmdline: '{}'", mod_cmdline);

                if !kernel_opts.is_empty() {
                    mod_cmdline.push(' ');
                    mod_cmdline.push_str(kernel_opts);
                }

                mod_cmdline.push('\n');
                trace!("cmdline: '{}'", mod_cmdline);
                mod_cmdline
            }
            Err(why) => {
                error!(
                    "failed to read boot file '{}', error: {:?}",
                    cmdline_path.display(),
                    why
                );
                return Err(MigError::displayed());
            }
        };

        // save the backup locations to s2_config
        if !boot_cfg_bckup.is_empty() {
            s2_cfg.set_boot_bckup(boot_cfg_bckup);
        }

        // Finally write stuff

        let mut config_file = File::create(&config_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Failed to open file '{}' for writing",
                config_path.display()
            ),
        ))?;

        config_file
            .write(config_str.as_bytes())
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed write to file '{}'", config_path.display()),
            ))?;

        let mut cmdline_file = File::create(&cmdline_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "Failed to open file '{}' for writing",
                cmdline_path.display()
            ),
        ))?;

        cmdline_file
            .write(cmdline_str.as_bytes())
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed write to file '{}'", cmdline_path.display()),
            ))?;

        // TODO: Optional backup & modify cmd_line.txt - eg. add debug

        Ok(())
    }

    fn restore(&self, _mounts: &Mounts, _config: &Stage2Config) -> bool {
        // TODO: remove kernel & initramfs, dtb  too
        false
    }
}
