use failure::ResultExt;
use log::{debug, error, info, trace, warn};
use regex::Regex;
use std::fs::{copy, create_dir_all, rename, File};
use std::io::Write;

const STARTUP_TEMPLATE: &str = r#"
echo -off
echo Starting balena Migrate Environment
"#;

use crate::{
    common::{
        boot_manager::BootManager,
        device_info::DeviceInfo,
        dir_exists, file_exists, format_size_with_unit,
        migrate_info::MigrateInfo,
        os_api::{OSApi, OSApiImpl},
        path_append,
        stage2_config::Stage2ConfigBuilder,
        Config, MigErrCtx, MigError, MigErrorKind,
    },
    defs::{BootType, EFI_STARTUP_FILE, MIG_INITRD_NAME, MIG_KERNEL_NAME},
    mswin::{
        drive_info::DriveInfo,
        msw_defs::{
            BALENA_EFI_DIR, EFI_BCKUP_DIR, EFI_BOOT_DIR, EFI_DEFAULT_BOOTMGR64, EFI_MS_BOOTMGR,
        },
    },
};

pub(crate) struct EfiBootManager {
    msw_device: bool,
    boot_device: Option<DeviceInfo>,
}

impl EfiBootManager {
    pub fn new() -> EfiBootManager {
        EfiBootManager {
            msw_device: true,
            boot_device: None,
        }
    }
}

impl BootManager for EfiBootManager {
    fn get_boot_type(&self) -> BootType {
        BootType::MSWEfi
    }

    fn can_migrate(
        &mut self,
        mig_info: &MigrateInfo,
        _config: &Config,
        _s2_cfg: &mut Stage2ConfigBuilder,
    ) -> Result<bool, MigError> {
        let drive_info = DriveInfo::new()?;

        let efi_drive = match DeviceInfo::for_efi() {
            Ok(efi_drive) => efi_drive,
            Err(why) => {
                error!("The EFI drive could not be found, error: {:?}", why);
                return Ok(false);
            }
        };

        // make sure work path drive can be mapped to linux drive
        if let None = mig_info.work_path.device_info.part_uuid {
            error!("Cowardly refusing to migrate work partition without partuuid. Windows to linux drive name mapping is insecure");
            return Ok(false);
        }

        // make sure efi drive can be mapped to linux drive
        if let None = efi_drive.part_uuid {
            // TODO: add option to override this
            error!("Cowardly refusing to migrate EFI partition without partuuid. Windows to linux drive name mapping is insecure");
            return Ok(false);
        }

        // TODO: get a better estimate for startup file size
        let efi_path = path_append(&efi_drive.mountpoint, BALENA_EFI_DIR);

        let mut required_space: u64 = if file_exists(path_append(
            &efi_path,
            mig_info.kernel_file.rel_path.as_ref().unwrap(),
        )) {
            0
        } else {
            mig_info.kernel_file.size
        };

        if !file_exists(path_append(
            &efi_path,
            mig_info.initrd_file.rel_path.as_ref().unwrap(),
        )) {
            required_space += mig_info.initrd_file.size;
        }

        if !file_exists(path_append(&efi_drive.mountpoint, EFI_STARTUP_FILE)) {
            // TODO: get a better estimate for startup file size
            required_space += 50;
        }

        if efi_drive.fs_free < required_space {
            error!("Not enough free space for boot setup found on EFI partition. {} of free space are required on EFI partition.", format_size_with_unit(required_space));
            return Ok(false);
        }

        self.boot_device = Some(efi_drive);

        Ok(true)
    }

    fn setup(
        &self,
        mig_info: &MigrateInfo,
        s2_cfg: &mut Stage2ConfigBuilder,
        kernel_opts: &str,
    ) -> Result<(), MigError> {
        trace!("setup: entered");
        // for now:
        // copy our kernel & initramfs to \EFI\balena-migrate
        // move all boot manager files in
        //    \EFI\Boot\bootx86.efi
        //    \EFI\Microsoft\Boot\bootmgrfw.efi
        // to a safe place and add a
        // create a startup.nsh file in \EFI\Boot\ that refers to our kernel & initramfs

        let boot_dev = if let Some(ref boot_dev) = self.boot_device {
            boot_dev
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                "boot_device not set for boot_manager",
            ));
        };

        debug!(
            "efi drive found, setting boot manager to '{}'",
            boot_dev.get_alt_path().display()
        );

        let balena_efi_dir = path_append(&boot_dev.mountpoint, BALENA_EFI_DIR);
        if !dir_exists(&balena_efi_dir)? {
            create_dir_all(&balena_efi_dir).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to create EFI directory '{}'",
                    balena_efi_dir.display()
                ),
            ))?;
        }

        let kernel_path = path_append(&balena_efi_dir, MIG_KERNEL_NAME);
        debug!(
            "copy '{}' to '{}'",
            &mig_info.kernel_file.path.display(),
            &kernel_path.display()
        );
        copy(&mig_info.kernel_file.path, &kernel_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to copy migrate kernel to EFI directory '{}'",
                kernel_path.display()
            ),
        ))?;
        let initrd_path = path_append(&balena_efi_dir, MIG_INITRD_NAME);
        debug!(
            "copy '{}' to '{}'",
            &mig_info.initrd_file.path.display(),
            &initrd_path.display()
        );
        copy(&mig_info.initrd_file.path, &initrd_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to copy migrate initramfs to EFI directory '{}'",
                initrd_path.display()
            ),
        ))?;

        let efi_boot_dir = path_append(&boot_dev.mountpoint, EFI_BOOT_DIR);
        if !dir_exists(&efi_boot_dir)? {
            create_dir_all(&balena_efi_dir).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to create EFI directory '{}'",
                    efi_boot_dir.display()
                ),
            ))?;
        }

        let startup_path = path_append(efi_boot_dir, EFI_STARTUP_FILE);

        debug!("writing '{}'", &startup_path.display());
        let drive_letter_re = Regex::new(r#"^[a-z,A-Z]:(.*)$"#).unwrap();
        let tmp_path = kernel_path.to_string_lossy();
        let kernel_path = if let Some(captures) = drive_letter_re.captures(&tmp_path) {
            captures.get(1).unwrap().as_str()
        } else {
            &tmp_path
        };
        let tmp_path = initrd_path.to_string_lossy();
        let initrd_path = if let Some(captures) = drive_letter_re.captures(&tmp_path) {
            captures.get(1).unwrap().as_str()
        } else {
            &tmp_path
        };

        // TODO: prefer PARTUUID to guessed device name

        let startup_content = if let Some(ref partuuid) = boot_dev.part_uuid {
            format!(
                "{}{} initrd={} root=PARTUUID={} rootfstype={} rootwait\n",
                STARTUP_TEMPLATE,
                kernel_path,
                OSApi::new()?.to_linux_path(initrd_path)?.to_string_lossy(),
                partuuid,
                boot_dev.fs_type
            )
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "No partuuid found for root device '{}'- cannot create root command",
                    boot_dev.device
                ),
            ));
        };

        let mut startup_file = File::create(&startup_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to create EFI startup file '{}'",
                startup_path.display()
            ),
        ))?;
        startup_file
            .write(startup_content.as_bytes())
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to write EFI startup file'{}'",
                    startup_path.display()
                ),
            ))?;

        // TODO: create fake EFI mountpoint and adapt backup paths to it
        let efi_bckup_dir = path_append(&boot_dev.mountpoint, EFI_BCKUP_DIR);
        if !dir_exists(&efi_bckup_dir)? {
            create_dir_all(&efi_bckup_dir).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to create EFI backup directory '{}'",
                    efi_bckup_dir.display()
                ),
            ))?;
        }

        let os_api = OSApi::new()?;
        let mut boot_backup: Vec<(String, String)> = Vec::new();
        let msw_boot_mgr = path_append(&boot_dev.mountpoint, EFI_MS_BOOTMGR);
        if file_exists(&msw_boot_mgr) {
            let backup_path = path_append(&efi_bckup_dir, &msw_boot_mgr.file_name().unwrap());
            info!(
                "backing up  '{}' to '{}'",
                &msw_boot_mgr.display(),
                backup_path.display()
            );
            rename(&msw_boot_mgr, &backup_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to create EFI backup for '{}'",
                    msw_boot_mgr.display()
                ),
            ))?;
            if let Ok(bckup_path) = os_api.to_linux_path(backup_path) {
                boot_backup.push((
                    String::from(&*os_api.to_linux_path(EFI_MS_BOOTMGR)?.to_string_lossy()),
                    String::from(&*os_api.to_linux_path(bckup_path)?.to_string_lossy()),
                ))
            } else {
                warn!("Failed to save backup for {}", EFI_DEFAULT_BOOTMGR64)
            }
        } else {
            info!(
                "not backing up  '{}' , file not found",
                &msw_boot_mgr.display()
            );
        }

        let os_api = OSApi::new()?;

        // TODO: allow 32 bit
        let def_boot_mgr = path_append(&boot_dev.mountpoint, EFI_DEFAULT_BOOTMGR64);
        if file_exists(&def_boot_mgr) {
            let backup_path = path_append(&efi_bckup_dir, &def_boot_mgr.file_name().unwrap());
            info!(
                "backing up  '{}' to '{}'",
                &def_boot_mgr.display(),
                backup_path.display()
            );
            rename(&def_boot_mgr, &backup_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to create EFI backup for '{}'",
                    def_boot_mgr.display()
                ),
            ))?;
            if let Ok(bckup_path) = os_api.to_linux_path(backup_path) {
                boot_backup.push((
                    String::from(
                        &*os_api
                            .to_linux_path(EFI_DEFAULT_BOOTMGR64)?
                            .to_string_lossy(),
                    ),
                    String::from(&*os_api.to_linux_path(bckup_path)?.to_string_lossy()),
                ))
            } else {
                warn!("Failed to save backup for {}", EFI_DEFAULT_BOOTMGR64)
            }
        } else {
            info!(
                "not backing up  '{}' , file not found",
                &def_boot_mgr.display()
            );
        }
        s2_cfg.set_boot_bckup(boot_backup);

        Ok(())
    }

    fn get_bootmgr_path(&self) -> DeviceInfo {
        self.boot_device.as_ref().unwrap().clone()
    }
}
