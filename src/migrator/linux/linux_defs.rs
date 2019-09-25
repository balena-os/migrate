pub const BOOT_PATH: &str = "/boot";
// pub const EFI_PATH: &str = "/boot/efi";
pub const ROOT_PATH: &str = "/";

pub const MIGRATE_LOG_FILE: &str = "migrate.log";

pub const MLO_FILE_NAME: &str = "MLO";
pub const UENV_FILE_NAME: &str = "uEnv.txt";
pub const UBOOT_FILE_NAME: &str = "u-boot.img";

pub const KERNEL_CMDLINE_PATH: &str = "/proc/cmdline";
pub const KERNEL_OSRELEASE_PATH: &str = "/proc/sys/kernel/osrelease";

pub const GRUB_CONFIG_DIR: &str = "/etc/grub.d";
pub const GRUB_CONFIG_FILE: &str = "/etc/grub.d/43_balena-migrate";
pub const GRUB_MIN_VERSION: &str = "2";

pub const SYS_UEFI_DIR: &str = "/sys/firmware/efi";

pub const NIX_NONE: Option<&'static [u8]> = None;

pub const STAGE2_MEM_THRESHOLD: u64 = 32 * 1024 * 1024; // 64 MiB

pub const PRE_PARTPROBE_WAIT_SECS: u64 = 5;
pub const POST_PARTPROBE_WAIT_SECS: u64 = 5;

pub const WHEREIS_CMD: &str = "whereis";
pub const CHMOD_CMD: &str = "chmod";
pub const DD_CMD: &str = "dd";
pub const DF_CMD: &str = "df";
//pub const FDISK_CMD: &str = "fdisk";
pub const FILE_CMD: &str = "file";
pub const LSBLK_CMD: &str = "lsblk";
//pub const BLKID_CMD: &str = "blkid";
pub const GRUB_REBOOT_CMD: &str = "grub-reboot";
pub const GRUB_UPDT_CMD: &str = "update-grub";
pub const GZIP_CMD: &str = "gzip";
pub const MKTEMP_CMD: &str = "mktemp";
pub const MOKUTIL_CMD: &str = "mokutil";
pub const MOUNT_CMD: &str = "mount";
pub const LOSETUP_CMD: &str = "losetup";
pub const PARTED_CMD: &str = "parted";
pub const PARTPROBE_CMD: &str = "partprobe";
pub const REBOOT_CMD: &str = "reboot";
pub const TAR_CMD: &str = "tar";
pub const UDEVADM_CMD: &str = "udevadm";
pub const UNAME_CMD: &str = "uname";
pub const EXT_FMT_CMD: &str = "mkfs.ext4";
pub const FAT_FMT_CMD: &str = "mkfs.vfat";

pub const FAT_CHK_CMD: &str = "fsck.vfat";

// TODO: EFI support in Linux
/*pub const BALENA_EFI_DIR: &str = r#"/EFI/balena-migrate"#;
pub const EFI_DEFAULT_BOOTMGR32: &str = r#"/EFI/Boot/bootx32.efi"#;
pub const EFI_DEFAULT_BOOTMGR64: &str = r#"/EFI/Boot/bootx64.efi"#;
pub const EFI_BOOT_DIR: &str = r#"/EFI/Boot"#;
pub const EFI_BCKUP_DIR: &str = r#"/efi_backup"#;
*/
