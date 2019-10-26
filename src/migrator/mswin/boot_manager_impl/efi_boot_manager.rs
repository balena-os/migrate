use failure::ResultExt;
use log::{debug, error, info, trace, warn};
use regex::Regex;
use std::fs::{copy, create_dir_all, rename, File};
use std::io::Write;

const SYSLINUX_CFG_TEMPLATE: &str = r#"
DEFAULT balena-migrate
LABEL balena-migrate
 SAY Now booting the balena kernel from SYSLINUX...
"#;

use crate::common::call;
use crate::defs::{EFI_SYSLINUX_CONFIG_FILE, MIG_SYSLINUX_LOADER_NAME};
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
    defs::{BootType, MIG_INITRD_NAME, MIG_KERNEL_NAME, MIG_SYSLINUX_NAME},
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
        s2_cfg: &mut Stage2ConfigBuilder,
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

        let balena_efi_path = path_append(&efi_drive.mountpoint, BALENA_EFI_DIR);

        // TODO: get a better estimate for startup file size

        let mut required_space: u64 = if file_exists(path_append(&balena_efi_path, MIG_KERNEL_NAME))
        {
            0
        } else {
            mig_info.kernel_file.size
        };

        if !file_exists(path_append(&balena_efi_path, MIG_INITRD_NAME)) {
            required_space += mig_info.initrd_file.size;
        }

        let syslinux = path_append(&mig_info.work_path.path, MIG_SYSLINUX_NAME);
        if !file_exists(&syslinux) {
            error!(
                "The syslinux executable '{}' could not be found",
                syslinux.display()
            );
            return Ok(false);
        } else {
            if !file_exists(path_append(&balena_efi_path, MIG_SYSLINUX_NAME)) {
                required_space += syslinux.metadata().unwrap().len();
            }
        }

        let syslinux_ldr = path_append(&mig_info.work_path.path, MIG_SYSLINUX_LOADER_NAME);
        if !file_exists(&syslinux_ldr) {
            error!(
                "The syslinux executable '{}' could not be found",
                syslinux_ldr.display()
            );
            return Ok(false);
        } else {
            if !file_exists(path_append(&balena_efi_path, MIG_SYSLINUX_LOADER_NAME)) {
                required_space += syslinux_ldr.metadata().unwrap().len();
            }
        }

        if !file_exists(path_append(&balena_efi_path, EFI_SYSLINUX_CONFIG_FILE)) {
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
        debug!("setup: entered");
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

        let syslinux_src = path_append(&mig_info.work_path.path, MIG_SYSLINUX_NAME);
        let syslinux_path = path_append(&balena_efi_dir, MIG_SYSLINUX_NAME);
        debug!(
            "copy '{}' to '{}'",
            &syslinux_src.display(),
            &syslinux_path.display()
        );
        copy(&syslinux_src, &syslinux_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to copy syslinux executable to EFI directory '{}'",
                syslinux_path.display()
            ),
        ))?;

        let sysldr_src = path_append(&mig_info.work_path.path, MIG_SYSLINUX_LOADER_NAME);
        let sysldr_path = path_append(&balena_efi_dir, MIG_SYSLINUX_LOADER_NAME);
        debug!(
            "copy '{}' to '{}'",
            &sysldr_src.display(),
            &sysldr_path.display()
        );
        copy(&sysldr_src, &sysldr_path).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "failed to copy syslinux loader executable to EFI directory '{}'",
                sysldr_path.display()
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

        let syslinux_cfg_path = path_append(balena_efi_dir, EFI_SYSLINUX_CONFIG_FILE);
        let os_api = OSApi::new()?;

        debug!("writing '{}'", &syslinux_cfg_path.display());

        let kernel_path = os_api.to_linux_path(kernel_path)?;
        let initrd_path = os_api.to_linux_path(initrd_path)?;

        // TODO: prefer PARTUUID to guessed device name

        let syslinux_cfg_content = if let Some(ref partuuid) = boot_dev.part_uuid {
            format!(
                "{} KERNEL {}\n APPEND ro root=PARTUUID={} rootfstype={} initrd={} rootwait\n",
                SYSLINUX_CFG_TEMPLATE,
                kernel_path.display(),
                partuuid,
                boot_dev.fs_type,
                initrd_path.display(),
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

        debug!("syslinux cfg: \n{}", syslinux_cfg_content);

        let mut syslinux_cfg_file =
            File::create(&syslinux_cfg_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to create syslinux cong=fig file '{}'",
                    syslinux_cfg_path.display()
                ),
            ))?;
        syslinux_cfg_file
            .write(syslinux_cfg_content.as_bytes())
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "Failed to write syslinux config file'{}'",
                    syslinux_cfg_path.display()
                ),
            ))?;

        let drive_letter_re = Regex::new(r#"^[a-z,A-Z]:(.*)$"#).unwrap();
        let tmp_path = syslinux_path.to_string_lossy();
        let syslinux_path = if let Some(captures) = drive_letter_re.captures(&tmp_path) {
            captures.get(1).unwrap().as_str()
        } else {
            &tmp_path
        };

        let cmdres = call(
            "BCDEdit",
            &["/set", "{bootmgr}", "path", &syslinux_path],
            true,
        )
        .context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            "failed to setup UEFI bootmanager as syslinux",
        ))?;

        debug!("BCDEDit result: '{}'", cmdres.stdout);

        if !cmdres.status.success() {
            return Err(MigError::from_remark(
                MigErrorKind::ExecProcess,
                &format!(
                    "failed to setup UEFI bootmanager as syslinux, message: {}",
                    cmdres.stderr
                ),
            ));
        }

        Ok(())
    }

    fn get_bootmgr_path(&self) -> DeviceInfo {
        self.boot_device.as_ref().unwrap().clone()
    }
}
