use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};

pub const BOOT_PATH: &str = "/boot";
// pub const EFI_PATH: &str = "/boot/efi";
pub const ROOT_PATH: &str = "/";

pub const MLO_FILE_NAME: &str = "MLO";
pub const UENV_FILE_NAME: &str = "uEnv.txt";
pub const UBOOT_FILE_NAME: &str = "u-boot.img";

pub const KERNEL_CMDLINE_PATH: &str = "/proc/cmdline";

pub const MIG_KERNEL_NAME: &str = "balena-migrate.zImage";
pub const MIG_INITRD_NAME: &str = "balena-migrate.initrd";
pub const MIG_DTB_NAME: &str = "balena-migrate.dtb";

pub const GRUB_CONFIG_DIR: &str = "/etc/grub.d";
pub const GRUB_CONFIG_FILE: &str = "/etc/grub.d/43_balena-migrate";
pub const GRUB_MIN_VERSION: &str = "2";

pub const SYS_UEFI_DIR: &str = "/sys/firmware/efi";

// where does the stage 2 config file live
pub const STAGE2_CFG_FILE: &str = "/etc/balena-stage2.yml";

// where do network manager connection profiles live
pub const SYSTEM_CONNECTIONS_DIR: &str = "system-connections";

// where do disk labels live ?
pub const DISK_BY_LABEL_PATH: &str = "/dev/disk/by-label";
pub const DISK_BY_PARTUUID_PATH: &str = "/dev/disk/by-partuuid";
pub const DISK_BY_UUID_PATH: &str = "/dev/disk/by-uuid";

// Default balena partition labels and FS types
pub const BALENA_BOOT_PART: &str = "resin-boot";
pub const BALENA_BOOT_FSTYPE: &str = "vfat";

pub const BALENA_ROOTA_PART: &str = "resin-rootA";
pub const BALENA_ROOTB_PART: &str = "resin-rootB";
pub const BALENA_STATE_PART: &str = "resin-state";

pub const BALENA_DATA_PART: &str = "resin-data";
pub const BALENA_DATA_FSTYPE: &str = "ext4";

// Default migrate config name
pub const DEFAULT_MIGRATE_CONFIG: &str = "balena-migrate.yml";

pub const NIX_NONE: Option<&'static [u8]> = None;

// tag files with this to determine they are written by balena-migrate
// and can be overwritten
pub const BALENA_FILE_TAG: &str = "## created by balena-migrate";
pub const BALENA_FILE_TAG_REGEX: &str = r###"^\s*## created by balena-migrate"###;

// balena config defaults
// pub const DEFAULT_API_HOST: &str = "api.balena-cloud.com";
// pub const DEFAULT_API_PORT: u16 = 443;
// check timeout used for API & VPN
pub const DEFAULT_API_CHECK_TIMEOUT: u64 = 20;

pub const BACKUP_FILE: &str = "backup.tgz";

pub const STAGE2_MEM_THRESHOLD: u64 = 32 * 1024 * 1024; // 64 MiB
pub const APPROX_MEM_THRESHOLD: u64 = 64 * 1024 * 1024; // 64 MiB

pub const MIN_DISK_SIZE: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) enum BootType {
    UBoot,
    Raspi,
    Efi,
    Grub,
}

#[derive(Debug,Clone)]
pub(crate) enum FileSystem {
    Ext2,
    Ext3,
    Ext4,
    VFat,
    Hpfs,
    Other
}

impl FileSystem {
    pub fn from_str(val: &str) -> FileSystem {
        match val.to_ascii_uppercase().as_ref() {
            "EXT2" => FileSystem::Ext2,
            "EXT3" => FileSystem::Ext3,
            "EXT4" => FileSystem::Ext4,
            "FAT" => FileSystem::VFat,
            "FAT32" => FileSystem::VFat,
            "FAT16" => FileSystem::VFat,
            "HPFS" => FileSystem::Hpfs,
            _ => FileSystem::Other
        }
    }
}


#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) enum DeviceType {
    BeagleboneGreen,
    BeagleboneBlack,
    BeagleboardXM,
    IntelNuc,
    RaspberryPi3,
}

#[derive(Debug, Clone)]
pub enum OSArch {
    AMD64,
    ARMHF,
    I386,
    /*
        ARM64,
        ARMEL,
        MIPS,
        MIPSEL,
        Powerpc,
        PPC64EL,
        S390EX,
    */
}

impl Display for OSArch {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) enum FailMode {
    Reboot,
    RescueShell,
}

impl FailMode {
    pub(crate) fn get_default() -> &'static FailMode {
        &FailMode::Reboot
    }
}
