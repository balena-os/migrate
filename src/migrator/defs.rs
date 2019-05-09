pub const BOOT_PATH: &str = "/boot";

// where does the stage 2 config file live
pub const STAGE2_CFG_FILE: &str = "/etc/balena-stage2.yml";

// where do network manager connection profiles live
pub const SYSTEM_CONNECTIONS_DIR: &str = "system-connections";

// where do disk labels live ?
pub const DISK_BY_LABEL_PATH: &str = "/dev/disk/by-label";

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

// tag files with this to determine they are written by balena-migrate
// and can be overwritten
pub const BALENA_FILE_TAG: &str = "## created by balena-migrate";
pub const BALENA_FILE_TAG_REGEX: &str = r###"^\s*## created by balena-migrate"###;

// balena config defaults
pub const DEFAULT_API_HOST: &str = "api.balena-cloud.com";
pub const DEFAULT_API_PORT: u16 = 443;
pub const DEFAULT_VPN_HOST: &str = "vpn.balena-cloud.com";
pub const DEFAULT_VPN_PORT: u16 = 443;
// check timeout used for API & VPN
pub const DEFAULT_API_CHECK_TIMEOUT: u64 = 20;
