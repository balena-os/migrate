use failure::{Fail, ResultExt};
use log::{info, trace, warn, error};
use regex::Regex;
use std::fs::{copy, File, read_to_string};
use std::io::{BufRead, BufReader, Write};

use std::time::SystemTime;

use crate::{
    common::{
        dir_exists, file_exists, is_balena_file, path_append,
        stage2_config::{Stage2Config, Stage2ConfigBuilder},
        Config, MigErrCtx, MigError, MigErrorKind,
    },
    defs::{BootType, BALENA_FILE_TAG},
    linux::{
        linux_defs::{BOOT_PATH},
        boot_manager::BootManager,
        stage2::mounts::{Mounts},
        EnsuredCmds,
        MigrateInfo, migrate_info::PathInfo,
        CHMOD_CMD},
};

const RPI_MIG_KERNEL_PATH: &str = "/boot/balena.zImage";
const RPI_MIG_KERNEL_NAME: &str = "balena.zImage";

const RPI_MIG_INITRD_PATH: &str = "/boot/balena.initramfs.cpio.gz";
const RPI_MIG_INITRD_NAME: &str = "balena.initramfs.cpio.gz";

const RPI_CONFIG_TXT: &str = "config.txt";
const RPI_CMDLINE_TXT: &str = "cmdline.txt";

pub(crate) struct RaspiBootManager {
    // valid is just used to enforce the use of new
    boot_path: Option<PathInfo>,
}

impl RaspiBootManager {
    pub fn new() -> RaspiBootManager {
        RaspiBootManager { boot_path: None }
    }
}

impl BootManager for RaspiBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::Raspi
    }

    fn get_boot_path(&self) -> PathInfo {
        self.boot_path.as_ref().unwrap().clone()
    }

    fn can_migrate(
        &mut self,
        cmds: &mut EnsuredCmds,
        mig_info: &MigrateInfo,
        _config: &Config,
        _s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError> {
        // TODO: calculate/ensure  required space on /boot /bootmgr

        if !dir_exists(BOOT_PATH)?  {
            error!("The /boot directory required for the raspi boot manager could not be found");
            return Ok(false);
        }

        self.boot_path = Some(PathInfo::new(cmds, BOOT_PATH, &mig_info.lsblk_info)?.unwrap());

        Ok(true)
    }

    fn setup(
        &self,
        cmds: &EnsuredCmds,
        _mig_info: &MigrateInfo,
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

        let boot_path = self.boot_path.as_ref().unwrap();
        let config_path = path_append(&boot_path.path, RPI_CONFIG_TXT);

        if !file_exists(&config_path) {
            return Err(MigError::from_remark(
                MigErrorKind::NotFound,
                &format!("Could not find '{}'", config_path.display()),
            ));
        }

        // create backup of config.txt

        let system_time = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                "Failed to create timestamp",
            ))?;

        let mut boot_cfg_bckup: Vec<(String, String)> = Vec::new();

        let balena_config = is_balena_file(&config_path)?;
        if !balena_config {
            // backup config.txt
            let backup_path = path_append(
                &boot_path.path,
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

            boot_cfg_bckup.push((
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
            let backup_path = path_append(
                &boot_path.path,
                &format!("{}.{}", RPI_CMDLINE_TXT, system_time.as_secs()),
            );

            copy(&cmdline_path, &backup_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to copy '{}' to '{}'",
                    cmdline_path.display(),
                    backup_path.display()
                ),
            ))?;

            boot_cfg_bckup.push((
                String::from(&*cmdline_path.to_string_lossy()),
                String::from(&*backup_path.to_string_lossy()),
            ));
        }

        let cmdline_str = match read_to_string(&cmdline_path) {
            Ok(cmdline) => {
                let cmdline = cmdline.trim_end();
                let root_cmd = format!("root={}", &boot_path.get_kernel_cmd());
                let rep: &str = root_cmd.as_ref();
                let mut mod_cmdline = String::from(Regex::new(r#"root=\S+"#).unwrap().replace(cmdline,rep));
                if !mod_cmdline.contains(rep) {
                    mod_cmdline.push(' ');
                    mod_cmdline.push_str(&root_cmd);
                }

                let rootfs_cmd = format!("rootfstype={}", &boot_path.fs_type);
                let rep: &str = rootfs_cmd.as_ref();
                mod_cmdline = String::from(Regex::new(r#"rootfstype=\S+"#).unwrap().replace(mod_cmdline.as_ref(),rep));
                if !mod_cmdline.contains(rep) {
                    mod_cmdline.push(' ');
                    mod_cmdline.push_str(&rootfs_cmd);
                }

                mod_cmdline.push('\n');
                mod_cmdline
            },
            Err(why) => {
                error!("failed to read boot file '{}'", cmdline_path.display());
                return Err(MigError::displayed());
            }
        };

        // save the backup loactions to s2_config
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

    fn restore(
        &self,
        _mounts: &Mounts,
        _config: &Stage2Config,
    ) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
}
