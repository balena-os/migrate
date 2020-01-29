use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;

// where does the stage 2 config file live
pub const STAGE2_CFG_FILE: &str = "balena-stage2.yml";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// where do network manager connection profiles live
pub const SYSTEM_CONNECTIONS_DIR: &str = "system-connections";

// Default migrate config name
pub const DEFAULT_MIGRATE_CONFIG: &str = "balena-migrate.yml";

#[allow(dead_code)]
pub const BALENA_EFI_DIR: &str = r#"\EFI\balena"#;
#[allow(dead_code)]
pub const EFI_BOOT_DIR: &str = r#"\EFI\Boot"#;

#[allow(dead_code)]
pub const MIG_SYSLINUX_EFI_NAME: &str = "syslinux.efi";
#[allow(dead_code)]
pub const MIG_SYSLINUX_LOADER_NAME_X64: &str = "ldlinux.e64";
#[allow(dead_code)]
pub const MIG_SYSLINUX_LOADER_NAME_IA32: &str = "ldlinux.e32";

#[allow(dead_code)]
pub const EFI_SYSLINUX_CONFIG_FILE_X64: &str = "syslx64.cfg";
#[allow(dead_code)]
pub const EFI_SYSLINUX_CONFIG_FILE_IA32: &str = "syslia32.cfg";
#[allow(dead_code)]
pub const EFI_SYSLINUX_CONFIG_FILE_ANY: &str = "syslinux.cfg";

pub const MIG_KERNEL_NAME: &str = "balena-migrate.zImage";
pub const MIG_INITRD_NAME: &str = "balena-migrate.initrd";

#[allow(dead_code)]
pub const APPROX_MEM_THRESHOLD: u64 = 1024 * 1024;

// where do disk labels live ?
pub const DISK_BY_LABEL_PATH: &str = "/dev/disk/by-label";
pub const DISK_BY_PARTUUID_PATH: &str = "/dev/disk/by-partuuid";
pub const DISK_BY_UUID_PATH: &str = "/dev/disk/by-uuid";

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

pub const MIN_DISK_SIZE: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB

#[allow(dead_code)]
pub const DEF_BLOCK_SIZE: usize = 512;

pub const STAGE1_MEM_THRESHOLD: u64 = 1024 * 1024 * 100; // 100 MB

// Default balena partition labels and FS types
// TODO:move to linux_defs
#[allow(dead_code)]
pub const BALENA_BOOT_PART: &str = "resin-boot";
#[allow(dead_code)]
pub const BALENA_BOOT_FSTYPE: &str = "vfat";

pub const BALENA_ROOTA_PART: &str = "resin-rootA";
pub const BALENA_ROOTA_FSTYPE: &str = "ext4";
pub const BALENA_ROOTB_PART: &str = "resin-rootB";
pub const BALENA_ROOTB_FSTYPE: &str = "ext4";
pub const BALENA_STATE_PART: &str = "resin-state";
pub const BALENA_STATE_FSTYPE: &str = "ext4";

pub const BALENA_API_PORT: u16 = 80;

pub const BALENA_DATA_PART: &str = "resin-data";
pub const BALENA_DATA_FSTYPE: &str = "ext4";

#[allow(dead_code)]
pub const PART_NAME: &[&str] = &[
    BALENA_BOOT_PART,
    BALENA_ROOTA_PART,
    BALENA_ROOTB_PART,
    BALENA_STATE_PART,
    BALENA_DATA_PART,
];
#[allow(dead_code)]
pub const PART_FSTYPE: &[&str] = &[
    BALENA_BOOT_FSTYPE,
    BALENA_ROOTA_FSTYPE,
    BALENA_ROOTB_FSTYPE,
    BALENA_STATE_FSTYPE,
    BALENA_DATA_FSTYPE,
];

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub(crate) enum BootType {
    UBoot,
    Raspi,
    Raspi64,
    Efi,
    Grub,
    MSWEfi,
    MSWBootMgr,
}

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
pub(crate) enum DeviceType {
    BeagleboneGreen,
    // BeagleboneBlack,
    BeagleboardXM,
    IntelNuc,
    RaspberryPi2,
    RaspberryPi3,
    RaspberryPi4_64,
}

#[derive(Debug, Clone)]
pub enum OSArch {
    AMD64,
    #[cfg(target_os = "linux")]
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) enum DeviceSpec {
    #[serde(rename = "uuid")]
    Uuid(String),
    #[serde(rename = "partuuid")]
    PartUuid(String),
    #[serde(rename = "devpath")]
    DevicePath(PathBuf),
    #[serde(rename = "path")]
    Path(PathBuf),
    #[serde(rename = "label")]
    Label(String),
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
#[derive(Debug, Clone)]
pub(crate) enum FileType {
    GZipOSImage,
    #[cfg(target_os = "linux")]
    OSImage,
    #[cfg(target_os = "linux")]
    KernelAMD64,
    #[cfg(target_os = "linux")]
    KernelARMHF,
    //    KernelI386,
    #[cfg(target_os = "linux")]
    KernelAARCH64,
    InitRD,
    Json,
    Text,
    // DTB,
    GZipTar,
}

impl FileType {
    #[cfg(target_os = "linux")]
    pub fn get_descr(&self) -> &str {
        match self {
            FileType::GZipOSImage => "gzipped balena OS image",
            FileType::OSImage => "balena OS image",
            FileType::KernelAMD64 => "balena migrate kernel image for AMD64",
            FileType::KernelARMHF => "balena migrate kernel image for ARMHF",
            //           FileType::KernelI386 => "balena migrate kernel image for I386",
            FileType::KernelAARCH64 => "balena migrate kernel image for AARCH64",
            FileType::InitRD => "balena migrate initramfs",
            // FileType::DTB => "Device Tree Blob",
            FileType::Json => "balena config.json file",
            FileType::Text => "Text file",
            FileType::GZipTar => "Gzipped Tar file",
        }
    }
}
