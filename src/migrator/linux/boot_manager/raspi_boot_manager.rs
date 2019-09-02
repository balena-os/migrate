use failure::{Fail, ResultExt};
use log::{debug, error, info, trace, warn};
use regex::Regex;
use std::fs::{copy, read_to_string, File};
use std::io::{BufRead, BufReader, Write};

use std::time::SystemTime;

use crate::{
    common::{
        dir_exists,
        file_digest::check_digest,
        file_exists, is_balena_file, path_append,
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigErrCtx, MigError, MigErrorKind,
    },
    defs::{BootType, BALENA_FILE_TAG},
    linux::{
        boot_manager::BootManager, linux_defs::BOOT_PATH, migrate_info::PathInfo,
        stage2::mounts::Mounts, EnsuredCmds, MigrateInfo, CHMOD_CMD,
    },
};

// TODO: copy rpi dtb's , backup orig dtbs

const RPI_MIG_KERNEL_PATH: &str = "/boot/balena.zImage";
const RPI_MIG_KERNEL_NAME: &str = "balena.zImage";

const RPI_MIG_INITRD_PATH: &str = "/boot/balena.initramfs.cpio.gz";
const RPI_MIG_INITRD_NAME: &str = "balena.initramfs.cpio.gz";

const RPI_CONFIG_TXT: &str = "config.txt";
const RPI_CMDLINE_TXT: &str = "cmdline.txt";
const RPI_BOOT_PATH: &str = "/boot";

// TODO: more specific lists for PRI types ?
const RPI_DTB_FILES: [&str; 8] = [
    "bcm2708-rpi-0-w.dtb",
    "bcm2708-rpi-b.dtb",
    "bcm2708-rpi-b-plus.dtb",
    "bcm2708-rpi-cm.dtb",
    "bcm2709-rpi-2-b.dtb",
    "bcm2710-rpi-3-b.dtb",
    "bcm2710-rpi-3-b-plus.dtb",
    "bcm2710-rpi-cm3.dtb",
];

pub(crate) struct RaspiBootManager {
    bootmgr_path: Option<PathInfo>,
}

impl RaspiBootManager {
    pub fn new() -> RaspiBootManager {
        RaspiBootManager { bootmgr_path: None }
    }
}

impl BootManager for RaspiBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::Raspi
    }

    fn get_bootmgr_path(&self) -> PathInfo {
        self.bootmgr_path.as_ref().unwrap().clone()
    }

    // TODO: do we need to distiguish like in u-boot ?
    fn get_boot_path(&self) -> PathInfo {
        self.bootmgr_path.as_ref().unwrap().clone()
    }

    fn can_migrate(
        &mut self,
        cmds: &mut EnsuredCmds,
        mig_info: &MigrateInfo,
        _config: &Config,
        _s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError> {
        // TODO: calculate/ensure  required space on /boot /bootmgr

        if !dir_exists(BOOT_PATH)? {
            error!("The /boot directory required for the raspi boot manager could not be found");
            return Ok(false);
        }

        self.bootmgr_path = Some(PathInfo::new(cmds, BOOT_PATH, &mig_info.lsblk_info)?.unwrap());

        // TODO: provide a way to supply digests for DTB files
        for file in &RPI_DTB_FILES {
            if !file_exists(path_append(&mig_info.work_path.path, file)) {
                error!(
                    "The file '{}' could not be found in the working directory",
                    file
                );
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn setup(
        &self,
        cmds: &EnsuredCmds,
        mig_info: &MigrateInfo,
        s2_cfg: &mut Stage2ConfigBuilder,
        kernel_opts: &str,
    ) -> Result<(), MigError> {
        trace!("setup: entered with type: RaspberryPi3",);

        // **********************************************************************
        // ** copy new kernel
        std::fs::copy(&mig_info.kernel_file.path, RPI_MIG_KERNEL_PATH).context(
            MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to copy kernel file '{}' to '{}'",
                    mig_info.kernel_file.path.display(),
                    RPI_MIG_KERNEL_PATH
                ),
            ),
        )?;

        if !check_digest(RPI_MIG_KERNEL_PATH, &mig_info.kernel_file.hash_info)? {
            return Err(MigError::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to check digest on copied kernel file '{}' to {:?}",
                    RPI_MIG_KERNEL_PATH, mig_info.kernel_file.hash_info
                ),
            ));
        }

        info!(
            "copied kernel: '{}' -> '{}'",
            mig_info.kernel_file.path.display(),
            RPI_MIG_KERNEL_PATH
        );

        cmds.call(CHMOD_CMD, &["+x", RPI_MIG_KERNEL_PATH], false)?;

        // **********************************************************************
        // ** copy new iniramfs
        std::fs::copy(&mig_info.initrd_file.path, RPI_MIG_INITRD_PATH).context(
            MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to copy initrd file '{}' to '{}'",
                    mig_info.initrd_file.path.display(),
                    RPI_MIG_INITRD_PATH
                ),
            ),
        )?;

        if !check_digest(RPI_MIG_INITRD_PATH, &mig_info.initrd_file.hash_info)? {
            return Err(MigError::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to check digest on copied initrd file '{}' to {:?}",
                    RPI_MIG_INITRD_PATH, mig_info.initrd_file.hash_info
                ),
            ));
        }

        info!(
            "copied initramfs: '{}' -> '{}'",
            mig_info.initrd_file.path.display(),
            RPI_MIG_INITRD_PATH
        );

        let boot_path = if let Some(ref boot_path) = self.bootmgr_path {
            boot_path
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                "bootmgr_path is not configured",
            ));
        };

        // create backup of config.txt

        let system_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                "Failed to create timestamp",
            ))?;

        let mut boot_cfg_bckup: Vec<(String, String)> = Vec::new();

        for file in &RPI_DTB_FILES {
            let src_path = path_append(&mig_info.work_path.path, file);
            let tgt_path = path_append(&RPI_BOOT_PATH, file);

            if file_exists(&tgt_path) {
                let backup_file = format!("{}-{}", file, system_time.as_secs());
                let backup_path = path_append(RPI_BOOT_PATH, &backup_file);
                copy(&tgt_path, &backup_path).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!(
                        "Failed to copy '{}' to '{}'",
                        tgt_path.display(),
                        backup_path.display()
                    ),
                ))?;
                boot_cfg_bckup.push((String::from(*file), backup_file));
            }

            copy(&src_path, &tgt_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to copy '{}' to '{}'",
                    src_path.display(),
                    tgt_path.display()
                ),
            ))?;

            if let Some(file_info) = mig_info.dtb_file.iter().find(|&file_info| {
                if let Some(ref rel_path) = file_info.rel_path {
                    &&*rel_path.to_string_lossy() == file
                } else {
                    false
                }
            }) {
                debug!("Found digest for '{}', checking ", file);
                match check_digest(&tgt_path, &file_info.hash_info) {
                    Ok(res) => {
                        if !res {
                            // TODO: implement rollback, return error
                            warn!("Digest did not match on '{}' proceeding anyway", file)
                        }
                    }
                    Err(why) => warn!(
                        "Failed to check digest on file '{}', error: {:?}, proceeding anyway",
                        file, why
                    ),
                }
            }
        }

        let config_path = path_append(&boot_path.path, RPI_CONFIG_TXT);

        if !file_exists(&config_path) {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("Could not find '{}'", config_path.display()),
            ));
        }

        let balena_config = is_balena_file(&config_path)?;
        if !balena_config {
            // backup config.txt
            let backup_file = format!("{}.{}", RPI_CONFIG_TXT, system_time.as_secs());
            let backup_path = path_append(&boot_path.path, &backup_file);

            copy(&config_path, &backup_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to copy '{}' to '{}'",
                    config_path.display(),
                    backup_path.display()
                ),
            ))?;

            boot_cfg_bckup.push((String::from(RPI_CONFIG_TXT), backup_file.clone()));

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

        let uart_re = Regex::new(r#"^\s*enable_uart"#).unwrap();
        let mut uart_found = false;

        let mut config_str = String::new();

        if !balena_config {
            config_str += &format!("{}\n", BALENA_FILE_TAG);
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
                            config_str.push_str(&format!("# {}\n", line));
                            if !initrd_found {
                                config_str.push_str(&format!(
                                    "initramfs {} followkernel\n",
                                    RPI_MIG_INITRD_NAME
                                ));
                                initrd_found = true;
                            }
                        } else if kernel_re.is_match(&line) {
                            // save commented version anyway
                            config_str.push_str(&format!("# {}\n", line));
                            if !kernel_found {
                                config_str.push_str(&format!("kernel {}\n", RPI_MIG_KERNEL_NAME));
                                kernel_found = true;
                            }
                        } else if uart_re.is_match(&line) {
                            config_str.push_str(&format!("# {}\n", line));
                            if !uart_found {
                                config_str.push_str("enable_uart=1\n");
                                uart_found = true;
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

        if !initrd_found {
            // add it if it did not exist
            config_str.push_str(&format!("initramfs {} followkernel\n", RPI_MIG_INITRD_NAME));
        }

        if !kernel_found {
            // add it if it did not exist
            config_str.push_str(&format!("kernel {}\n", RPI_MIG_KERNEL_NAME));
        }

        info!(
            "Modified '{}' to boot migrate environment",
            config_path.display()
        );

        let cmdline_path = path_append(&boot_path.path, RPI_CMDLINE_TXT);
        // Assume we have to backup cmdline.txt if we had to backup config.txt
        if !balena_config {
            // backup cmdline.txt
            let backup_file = format!("{}.{}", RPI_CMDLINE_TXT, system_time.as_secs());
            let backup_path = path_append(&boot_path.path, &backup_file);

            copy(&cmdline_path, &backup_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to copy '{}' to '{}'",
                    cmdline_path.display(),
                    backup_path.display()
                ),
            ))?;

            boot_cfg_bckup.push((String::from(RPI_CMDLINE_TXT), backup_file.clone()));
        }

        let cmdline_str = match read_to_string(&cmdline_path) {
            Ok(cmdline) => {
                let cmdline = cmdline.trim_end();

                trace!("cmdline: '{}'", cmdline);
                // Add or replace root command to cmdline
                let cmd_fragment = format!(" root={} ", &boot_path.get_kernel_cmd());
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
                let cmd_fragment = format!(" rootfstype={} ", &boot_path.fs_type);
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
                mod_cmdline.push_str(&format!(" console=tty1 console=serial0,115200"));

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
        if boot_cfg_bckup.len() > 0 {
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
