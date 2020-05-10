pub const BALENA_BOOT_PART: &str = "resin-boot";
pub const BALENA_BOOT_FSTYPE: &str = "vfat";

pub const BALENA_ROOTA_PART: &str = "resin-rootA";
pub const BALENA_ROOTA_FSTYPE: &str = "ext4";
pub const BALENA_ROOTB_PART: &str = "resin-rootB";
pub const BALENA_ROOTB_FSTYPE: &str = "ext4";
pub const BALENA_STATE_PART: &str = "resin-state";
pub const BALENA_STATE_FSTYPE: &str = "ext4";

pub const BALENA_DATA_PART: &str = "resin-data";
pub const BALENA_DATA_FSTYPE: &str = "ext4";

pub const PART_NAME: &[&str] = &[
    BALENA_BOOT_PART,
    BALENA_ROOTA_PART,
    BALENA_ROOTB_PART,
    BALENA_STATE_PART,
    BALENA_DATA_PART,
];

pub const PART_FSTYPE: &[&str] = &[
    BALENA_BOOT_FSTYPE,
    BALENA_ROOTA_FSTYPE,
    BALENA_ROOTB_FSTYPE,
    BALENA_STATE_FSTYPE,
    BALENA_DATA_FSTYPE,
];
