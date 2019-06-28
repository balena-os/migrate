pub const BOOT_PATH: &str = "/boot";
// pub const EFI_PATH: &str = "/boot/efi";
pub const ROOT_PATH: &str = "/";

pub const MIGRATE_LOG_FILE: &str = "migrate.log";

pub const MLO_FILE_NAME: &str = "MLO";
pub const UENV_FILE_NAME: &str = "uEnv.txt";
pub const UBOOT_FILE_NAME: &str = "u-boot.img";

pub const KERNEL_CMDLINE_PATH: &str = "/proc/cmdline";

pub const GRUB_CONFIG_DIR: &str = "/etc/grub.d";
pub const GRUB_CONFIG_FILE: &str = "/etc/grub.d/43_balena-migrate";
pub const GRUB_MIN_VERSION: &str = "2";

pub const SYS_UEFI_DIR: &str = "/sys/firmware/efi";

pub const NIX_NONE: Option<&'static [u8]> = None;

// Default balena partition labels and FS types
pub const BALENA_BOOT_PART: &str = "resin-boot";
pub const BALENA_BOOT_FSTYPE: &str = "vfat";

pub const BALENA_ROOTA_PART: &str = "resin-rootA";
pub const BALENA_ROOTB_PART: &str = "resin-rootB";
pub const BALENA_STATE_PART: &str = "resin-state";

pub const BALENA_DATA_PART: &str = "resin-data";
pub const BALENA_DATA_FSTYPE: &str = "ext4";

pub const STAGE2_MEM_THRESHOLD: u64 = 32 * 1024 * 1024; // 64 MiB

// TODO: EFI support in Linux
/*pub const BALENA_EFI_DIR: &str = r#"/EFI/balena-migrate"#;
pub const EFI_DEFAULT_BOOTMGR32: &str = r#"/EFI/Boot/bootx32.efi"#;
pub const EFI_DEFAULT_BOOTMGR64: &str = r#"/EFI/Boot/bootx64.efi"#;
pub const EFI_BOOT_DIR: &str = r#"/EFI/Boot"#;
pub const EFI_BCKUP_DIR: &str = r#"/efi_backup"#;
*/