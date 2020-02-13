use failure::ResultExt;
use lazy_static::lazy_static;
use log::{debug, error};
#[cfg(target_os = "linux")]
use log::{info, trace, warn};
use regex::Regex;
#[cfg(target_os = "linux")]
use std::fs::rename;
use std::fs::{copy, create_dir_all, File};
use std::io::Write;

const SYSLINUX_CFG_TEMPLATE: &str = r#"
DEFAULT balena-migrate
LABEL balena-migrate
 SAY Now booting the balena kernel from SYSLINUX...
"#;

use crate::common::call;
use crate::defs::{EFI_SYSLINUX_CONFIG_FILE_X64, MIG_SYSLINUX_LOADER_NAME_X64};
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
    defs::{
        BootType, BALENA_EFI_DIR, EFI_BOOT_DIR, MIG_INITRD_NAME, MIG_KERNEL_NAME,
        MIG_SYSLINUX_EFI_NAME,
    },
};
use crate::common::path_info::PathInfo;

#[allow(dead_code)]
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

impl EfiBootManager {
    fn bcd_edit(params: &[&str], parse_id: bool) -> Result<Option<String>, MigError> {
        lazy_static! {
            static ref BCD_ID_RE: Regex = Regex::new(r#"^The entry (\{[a-z,0-9]{8}-[a-z,0-9]{4}-[a-z,0-9]{4}-[a-z,0-9]{4}-[a-z,0-9]{12}\}) was successfully created.$"#).unwrap();
            static ref BCD_OK_RE: Regex = Regex::new(r#"The operation completed successfully."#).unwrap();
        }

        debug!("calling bcdedit with {:?}", params);
        let cmdres = call("BCDEdit", params, true).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            "Failed to execute bcdedit",
        ))?;

        if cmdres.status.success() {
            debug!("BCDEDit result: '{}'", cmdres.stdout);
            if parse_id {
                let bcd_id = if let Some(captures) = BCD_ID_RE.captures(&cmdres.stdout) {
                    captures.get(1).unwrap().as_str()
                } else {
                    error!(
                        "Failed to parse bcd id from bcdedit output '{}'",
                        cmdres.stdout
                    );
                    return Err(MigError::displayed());
                };

                Ok(Some(String::from(bcd_id)))
            } else {
                if BCD_OK_RE.is_match(&cmdres.stdout) {
                    Ok(None)
                } else {
                    error!(
                        "Failed to parse bcdedit success message from '{}'",
                        cmdres.stdout
                    );
                    return Err(MigError::displayed());
                }
            }
        } else {
            error!("bcdedit failed with message: '{}'", cmdres.stderr);
            return Err(MigError::displayed());
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
        // find / mount the efi drive
        let efi_drive = match DeviceInfo::for_efi() {
            Ok(efi_drive) => efi_drive,
            Err(why) => {
                error!("The EFI drive could not be found, error: {:?}", why);
                return Ok(false);
            }
        };

        // make sure efi drive can be mapped to linux drive
        if let None = efi_drive.part_uuid {
            // TODO: add option to override this
            error!("Cowardly refusing to migrate EFI partition without partuuid. Windows to linux drive name mapping is insecure");
            return Ok(false);
        }

        // make sure work path drive can be mapped to linux drive
        if let None = mig_info.work_path.device_info.part_uuid {
            error!("Cowardly refusing to migrate work partition without partuuid. Windows to linux drive name mapping is insecure");
            return Ok(false);
        }

        let balena_efi_path = path_append(&efi_drive.mountpoint, BALENA_EFI_DIR);

        let mut required_space: u64 = if file_exists(path_append(&balena_efi_path, MIG_KERNEL_NAME))
        {
            0
        } else {
            mig_info.kernel_file.size
        };

        if !file_exists(path_append(&balena_efi_path, MIG_INITRD_NAME)) {
            required_space += mig_info.initrd_file.size;
        }

        let syslinux = path_append(&mig_info.work_path.path, MIG_SYSLINUX_EFI_NAME);
        if !file_exists(&syslinux) {
            error!(
                "The syslinux executable '{}' could not be found",
                syslinux.display()
            );
            return Ok(false);
        } else {
            if !file_exists(path_append(&balena_efi_path, MIG_SYSLINUX_EFI_NAME)) {
                required_space += syslinux.metadata().unwrap().len();
            }
        }

        let syslinux_ldr = path_append(&mig_info.work_path.path, MIG_SYSLINUX_LOADER_NAME_X64);
        if !file_exists(&syslinux_ldr) {
            error!(
                "The syslinux loader '{}' could not be found",
                syslinux_ldr.display()
            );
            return Ok(false);
        } else {
            if !file_exists(path_append(&balena_efi_path, MIG_SYSLINUX_LOADER_NAME_X64)) {
                required_space += syslinux_ldr.metadata().unwrap().len();
            }
        }

        if !file_exists(path_append(&balena_efi_path, EFI_SYSLINUX_CONFIG_FILE_X64)) {
            // TODO: get a better estimate for startup file size
            // TODO: do we need a backup for this ?
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
        &mut self,
        mig_info: &MigrateInfo,
        config: &Config,
        _s2_cfg: &mut Stage2ConfigBuilder,
        kernel_opts: &str,
    ) -> Result<(), MigError> {
        debug!("setup: entered");
        // TODO: update this
        // for now:
        // copy our kernel & initramfs to \EFI\balena-migrate
        // copy our syslinux.efi & loader  to \EFI\balena-migrate
        // create syslinux config file in \EFI\balena-migrate
        // move all boot manager files in
        //    \EFI\Boot\bootx86.efi
        //    \EFI\Microsoft\Boot\bootmgrfw.efi
        // to a safe place and add a
        // create a startup.nsh file in \EFI\Boot\ that refers to our kernel & initramfs

        let efi_device = if let Some(ref boot_dev) = self.boot_device {
            boot_dev
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                "boot_device not set for boot_manager",
            ));
        };

        debug!(
            "efi drive found, setting boot manager to '{}'",
            efi_device.get_alt_path().display()
        );

        let balena_efi_dir = path_append(&efi_device.mountpoint, BALENA_EFI_DIR);
        if !dir_exists(&balena_efi_dir)? {
            create_dir_all(&balena_efi_dir).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to create EFI directory '{}'",
                    balena_efi_dir.display()
                ),
            ))?;
        }

        // TODO: check digest after file copies

        let kernel_path = path_append(&balena_efi_dir, MIG_KERNEL_NAME);
        debug!(
            "copy '{}' to '{}'",
            &mig_info.kernel_file.path.display(),
            &kernel_path.display()
        );
        // TODO: check digest after copy ?
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

        let syslinux_src = path_append(&mig_info.work_path.path, MIG_SYSLINUX_EFI_NAME);
        let syslinux_path = path_append(&balena_efi_dir, MIG_SYSLINUX_EFI_NAME);
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

        let sysldr_src = path_append(&mig_info.work_path.path, MIG_SYSLINUX_LOADER_NAME_X64);
        let sysldr_path = path_append(&balena_efi_dir, MIG_SYSLINUX_LOADER_NAME_X64);
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

        let efi_boot_dir = path_append(&efi_device.mountpoint, EFI_BOOT_DIR);
        if !dir_exists(&efi_boot_dir)? {
            create_dir_all(&balena_efi_dir).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!(
                    "failed to create EFI directory '{}'",
                    efi_boot_dir.display()
                ),
            ))?;
        }

        // create syslinux config file
        let syslinux_cfg_path = path_append(balena_efi_dir, EFI_SYSLINUX_CONFIG_FILE_X64);
        let os_api = OSApiImpl::new()?;

        debug!("writing '{}'", &syslinux_cfg_path.display());

        let kernel_path = os_api.to_linux_path(kernel_path)?;
        let initrd_path = os_api.to_linux_path(initrd_path)?;

        // TODO: prefer PARTUUID to guessed device name

        let syslinux_cfg_content = if let Some(ref partuuid) = efi_device.part_uuid {
            format!(
                "{} KERNEL {}\n APPEND ro root=PARTUUID={} rootfstype={} initrd={} rootwait {}\n",
                SYSLINUX_CFG_TEMPLATE,
                kernel_path.display(),
                partuuid,
                efi_device.fs_type,
                initrd_path.display(),
                kernel_opts
            )
        } else {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "No partuuid found for root device '{}'- cannot create root command",
                    efi_device.device
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

        // get relative (no driveletter) path to syslinux.efi
        let drive_letter_re = Regex::new(r#"^[a-z,A-Z]:(.*)$"#).unwrap();
        let tmp_path = syslinux_path.to_string_lossy();
        let syslinux_path = if let Some(captures) = drive_letter_re.captures(&tmp_path) {
            captures.get(1).unwrap().as_str()
        } else {
            &tmp_path
        };

        if config.debug.get_hack("bcd_add_menu").is_some() {
            // TODO: wip - preferable but not working yet
            let efi_drive_letter = &*efi_device.mountpoint.to_string_lossy();

            // create a new BCD entry and retrieve BCD ID
            // bcdedit /create /d "balena-migrate" /application startup
            //TODO: try to check if entry exists first
            let bcd_id = EfiBootManager::bcd_edit(
                &["/create", "/d", "balena-migrate", "/application", "startup"],
                true,
            )
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                "Failed to create new BCD entry",
            ))?
            .unwrap();

            debug!("Created new BCD entry with ID: {}", bcd_id);

            EfiBootManager::bcd_edit(
                &[
                    "/set",
                    &bcd_id,
                    "device",
                    &format!("partition={}", efi_drive_letter),
                ],
                false,
            )
            .context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                "Failed to set BCD entry device",
            ))?;

            debug!("BCD device set to partition={}", efi_drive_letter);

            EfiBootManager::bcd_edit(&["/set", &bcd_id, "path", syslinux_path], false).context(
                MigErrCtx::from_remark(MigErrorKind::Upstream, "Failed to set BCD entry path"),
            )?;

            debug!("BCD path set to {}", syslinux_path);

            // TODO: disable this in production
            EfiBootManager::bcd_edit(&["/displayorder", "{current}", &bcd_id], false).context(
                MigErrCtx::from_remark(MigErrorKind::Upstream, "Failed to activate BCD entry"),
            )?;
            debug!("BCD displayorder set - made new entry persistent",);

            EfiBootManager::bcd_edit(&["/bootsequence", &bcd_id, "{current}"], false).context(
                MigErrCtx::from_remark(MigErrorKind::Upstream, "Failed to activate BCD entry"),
            )?;

            debug!("One-Time-Activated new BCD entry {}", bcd_id);
        } else {
            // TODO: try enabling syslinux bootmanager manually instead of this radical solution
            EfiBootManager::bcd_edit(&["/set", "{bootmgr}", "path", &syslinux_path], false)
                .context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    "Failed to activate BCD entry",
                ))?;
        }

        Ok(())
    }

    fn get_bootmgr_path(&self) -> PathInfo {

        self.boot_device.as_ref().unwrap().clone()
    }
}
