#[derive(Debug,Clone)]
pub(crate) enum FileSystem {
    Ext2,
    Ext3,
    Ext4,
    VFat,
    Ntfs,
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
            "NTFS" => FileSystem::Ntfs,
            _ => FileSystem::Other
        }
    }

    pub fn to_linux_str(&self) -> &'static str {
        match self {
            FileSystem::Ext2 => "ext2",
            FileSystem::Ext3 => "ext3",
            FileSystem::Ext4 => "ext4",
            FileSystem::VFat => "vfat",
            FileSystem::Ntfs => "ntfs",
            _ => ""
        }
    }
}
