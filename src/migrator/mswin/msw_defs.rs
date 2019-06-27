pub const EFI_MS_BOOTMGR: &str = r#"\EFI\Microsoft\Boot\bootmgfw.efi"#;

pub const BALENA_EFI_DIR: &str = r#"\EFI\balena-migrate"#;
pub const EFI_DEFAULT_BOOTMGR32: &str = r#"\EFI\Boot\bootx32.efi"#;
pub const EFI_DEFAULT_BOOTMGR64: &str = r#"\EFI\Boot\bootx64.efi"#;
pub const EFI_BOOT_DIR: &str = r#"\EFI\Boot"#;
pub const EFI_BCKUP_DIR: &str = r#"\efi_backup"#;



#[derive(Debug, Clone)]
pub(crate) enum FileSystem {
    Ext2,
    Ext3,
    Ext4,
    VFat,
    Ntfs,
    Other,
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
            "NTFS" => FileSystem::Ntfs,
            _ => FileSystem::Other,
        }
    }

    pub fn to_linux_str(&self) -> &'static str {
        match self {
            FileSystem::Ext2 => "ext2",
            FileSystem::Ext3 => "ext3",
            FileSystem::Ext4 => "ext4",
            FileSystem::VFat => "vfat",
            FileSystem::Ntfs => "ntfs",
            _ => "",
        }
    }
}
