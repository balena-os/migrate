use log::{debug, error, trace};
use std::io::{Error, ErrorKind, Read};
use std::mem;
use std::path::{Path, PathBuf};

use crate::{
    common::{MigError, MigErrorKind},
    defs::DEF_BLOCK_SIZE,
};

mod image_file;
pub(crate) use image_file::ImageFile;

#[cfg(target_os = "linux")]
mod gzip_file;
#[cfg(target_os = "linux")]
pub(crate) use gzip_file::GZipFile;

mod plain_file;
pub(crate) use plain_file::PlainFile;

// TODO: implement GPT partition

#[derive(Debug)]
pub(crate) enum LabelType {
    GPT,
    Dos,
    Other,
}

impl LabelType {
    pub fn from_device<P: AsRef<Path>>(device_path: P) -> Result<LabelType, MigError> {
        let device_path = device_path.as_ref();
        // TODO: provide propper device block size
        Ok(Disk::from_drive_file(device_path, None)?.get_label()?)
    }
}

#[derive(Debug)]
pub(crate) enum PartitionType {
    Container,
    Fat,
    Linux,
    Empty,
    GPT,
    Other,
}

impl PartitionType {
    pub fn from_ptype(ptype: u8) -> PartitionType {
        // TODO: to be completed - currently only contains common, known partition types occurring in
        // encountered systems
        match ptype {
            0x00 => PartitionType::Empty,
            0x05 | 0x0f => PartitionType::Container,
            0xee => PartitionType::GPT,
            0x0c | 0x0e => PartitionType::Fat,
            0x83 => PartitionType::Linux,
            _ => PartitionType::Other,
        }
    }
}

#[repr(C, packed)]
struct PartEntry {
    status: u8,
    first_head: u8,
    first_comb: u8,
    first_cyl: u8,
    ptype: u8,
    last_head: u8,
    last_comb: u8,
    last_cyl: u8,
    first_lba: u32,
    num_sectors: u32,
}

#[repr(C, packed)]
struct MasterBootRecord {
    boot_code_0: [u8; 218],
    zeros: [u8; 2],
    orig_phys_drive: u8,
    drive_ts_seconds: u8,
    drive_ts_minutes: u8,
    drive_ts_hours: u8,
    boot_code_1: [u8; 216],
    disk_sig_32: [u8; 4],
    disk_sig_2: [u8; 2],
    part_tbl: [PartEntry; 4],
    boot_sig1: u8,
    boot_sig2: u8,
}

impl MasterBootRecord {
    pub fn get_disk_id(&self) -> Option<u32> {
        if self.zeros[0] == 0 && self.zeros[1] == 0 {
            let mut disk_sig_32: u32 = 0;
            for byte in self.disk_sig_32.iter().rev() {
                disk_sig_32 = disk_sig_32 * 256 + u32::from(*byte);
            }
            if disk_sig_32 != 0 {
                Some(disk_sig_32)
            } else {
                None
            }
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PartInfo {
    pub index: usize,
    pub ptype: u8,
    pub status: u8,
    pub start_lba: u64,
    pub num_sectors: u64,
}
impl PartInfo {
    pub fn is_bootable(&self) -> bool {
        (self.status & 0x80) == 0x80
    }
}

pub(crate) struct Disk {
    disk: Box<dyn ImageFile>,
    // writable: bool,
    block_size: u64,
}

impl Disk {
    #[cfg(target_os = "linux")]
    pub fn from_gzip_img<P: AsRef<Path>>(image: P) -> Result<Disk, MigError> {
        Ok(Disk {
            disk: Box::new(GZipFile::new(image.as_ref())?),
            // writable: false,
            block_size: DEF_BLOCK_SIZE as u64,
        })
    }

    pub fn from_drive_file<P: AsRef<Path>>(
        drive: P,
        // writable: bool,
        block_size: Option<u64>,
    ) -> Result<Disk, MigError> {
        Ok(Disk {
            disk: Box::new(PlainFile::new(drive.as_ref())?),
            // writable,
            block_size: if let Some(block_size) = block_size {
                block_size
            } else {
                DEF_BLOCK_SIZE as u64
            },
        })
    }

    pub fn get_image_file(&self) -> PathBuf {
        self.disk.get_path()
    }

    pub fn get_label(&mut self) -> Result<LabelType, MigError> {
        match self.read_mbr(0) {
            Ok(mbr) => match PartitionType::from_ptype(mbr.part_tbl[0].ptype) {
                PartitionType::GPT => Ok(LabelType::GPT),
                _ => Ok(LabelType::Dos),
            },
            Err(why) => {
                if why.kind() == MigErrorKind::InvParam {
                    Ok(LabelType::Other)
                } else {
                    Err(why)
                }
            }
        }
    }

    fn read_mbr(&mut self, block_idx: u64) -> Result<MasterBootRecord, MigError> {
        let mut buffer: [u8; DEF_BLOCK_SIZE] = [0; DEF_BLOCK_SIZE];

        self.disk
            .fill(block_idx * DEF_BLOCK_SIZE as u64, &mut buffer)?;

        let mbr: MasterBootRecord = unsafe { mem::transmute(buffer) };

        if (mbr.boot_sig1 != 0x55) || (mbr.boot_sig2 != 0xAA) {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "Encountered an invalid MBR signature, expected 0x55, 0xAA,  got {:x}, {:x}",
                    mbr.boot_sig1, mbr.boot_sig2
                ),
            ));
        }

        Ok(mbr)
    }
}

pub(crate) struct PartitionIterator<'a> {
    disk: &'a mut Disk,
    mbr: Option<MasterBootRecord>,
    offset: u64,
    index: usize,
    part_idx: usize,
    disk_id: Option<u32>,
}

impl<'a> PartitionIterator<'a> {
    pub fn new(disk: &mut Disk) -> Result<PartitionIterator, MigError> {
        let offset: u64 = 0;
        let mbr = disk.read_mbr(offset)?;
        let disk_id = mbr.get_disk_id();

        Ok(PartitionIterator {
            disk,
            mbr: Some(mbr),
            offset,
            index: 0,
            part_idx: 0,
            disk_id,
        })
    }

    #[allow(dead_code)]
    pub fn get_disk_id(&'a self) -> &'a Option<u32> {
        &self.disk_id
    }
}

// TODO: make functions for partition type:
// is extended
// is None
// is regular

impl<'a> Iterator for PartitionIterator<'a> {
    type Item = PartInfo;

    #[allow(clippy::cognitive_complexity)] //TODO refactor this function to fix the clippy warning
    fn next(&mut self) -> Option<Self::Item> {
        trace!("PartitionIterator::next: entered");
        // TODO: check for 0 size partition ?

        #[allow(clippy::large_enum_variant)] //TODO refactor to remove clippy warning
        enum SetMbr {
            Leave,
            ToNone,
            ToMbr(MasterBootRecord),
        }

        let (res, mbr) = if let Some(ref mbr) = self.mbr {
            if self.offset == 0 {
                debug!(
                    "PartitionIterator::next: offset: {}, index: {}, part_idx: {}, mbr: present",
                    self.offset, self.index, self.part_idx
                );
                // we are on the first partition table
                if self.index > 3 {
                    // end of regular partition table reached
                    (None, SetMbr::Leave)
                } else {
                    // read regular partition
                    let part = &mbr.part_tbl[self.index];
                    match PartitionType::from_ptype(part.ptype) {
                        PartitionType::Empty =>
                        // empty partition - Assume End of Table
                        {
                            (None, SetMbr::Leave)
                        }
                        PartitionType::Container => {
                            // extended / container
                            // return extended partition
                            self.offset = u64::from(part.first_lba);
                            // self.mbr = None; // we are done with this mbr
                            self.part_idx += 1;

                            (
                                Some(PartInfo {
                                    index: self.part_idx,
                                    ptype: part.ptype,
                                    status: part.status,
                                    start_lba: u64::from(part.first_lba),
                                    num_sectors: u64::from(part.num_sectors),
                                }),
                                SetMbr::ToNone,
                            )
                        }
                        PartitionType::Fat | PartitionType::Linux => {
                            // return regular partition
                            self.index += 1;
                            self.part_idx += 1;

                            (
                                Some(PartInfo {
                                    index: self.part_idx,
                                    ptype: part.ptype,
                                    status: part.status,
                                    start_lba: u64::from(part.first_lba),
                                    num_sectors: u64::from(part.num_sectors),
                                }),
                                SetMbr::Leave,
                            )
                        }
                        _ => {
                            error!("Unsupported partition type encountered: {:x}", part.ptype);
                            (None, SetMbr::Leave)
                        }
                    }
                }
            } else {
                // we are on an extended partitions table
                if self.index != 1 {
                    error!("Unexpected index into extended partition {}", self.index);
                    (None, SetMbr::Leave)
                } else {
                    // Extended partition tables should have only 2 entries. The actual partition
                    // which has already been reported (see None = self.mbr and below) and a pointer
                    // to the next extended partition which we would be looking at here

                    let part = &mbr.part_tbl[self.index];
                    match PartitionType::from_ptype(part.ptype) {
                        PartitionType::Empty => {
                            // regular end  of extended partitions
                            // // warn!("Empty partition on index 1 of extended partition is unexpected");
                            (None, SetMbr::Leave)
                        } // weird though
                        PartitionType::Container => {
                            // we are expecting a container partition here
                            self.offset += u64::from(part.first_lba);
                            match self.disk.read_mbr(self.offset) {
                                Ok(mbr) => {
                                    let part = &mbr.part_tbl[0];
                                    // self.mbr = Some(mbr)
                                    match PartitionType::from_ptype(part.ptype) {
                                        PartitionType::Linux | PartitionType::Fat => {
                                            self.index = 1;
                                            self.part_idx += 1;
                                            (
                                                Some(PartInfo {
                                                    index: self.part_idx,
                                                    ptype: part.ptype,
                                                    status: part.status,
                                                    start_lba: self.offset
                                                        + u64::from(part.first_lba),
                                                    num_sectors: u64::from(part.num_sectors),
                                                }),
                                                SetMbr::ToMbr(mbr),
                                            )
                                        }
                                        _ => {
                                            error!("Unexpected partition type {:x} on index 0 of extended partition", part.ptype);
                                            (None, SetMbr::Leave)
                                        }
                                    }
                                }
                                Err(why) => {
                                    error!("Failed to read mbr, error:{:?}", why);
                                    (None, SetMbr::Leave)
                                }
                            }
                        }
                        _ => {
                            error!(
                                "Unexpected partition type {:x} on index 1 of extended partition",
                                part.ptype
                            );
                            (None, SetMbr::Leave)
                        }
                    }
                }
            }
        } else {
            // this only happens after the first extended partition has been reported
            debug!(
                "PartitionIterator::next: offset: {}, index: {}, part_idx: {}, mbr: absent",
                self.offset, self.index, self.part_idx
            );
            match self.disk.read_mbr(self.offset) {
                Ok(mbr) => {
                    debug!("PartitionIterator::next: got mbr");
                    let part = &mbr.part_tbl[0];
                    // self.mbr = Some(mbr);
                    let part_type = PartitionType::from_ptype(part.ptype);
                    debug!(
                        "PartitionIterator::next: got partition type: {:?}",
                        part_type
                    );
                    match part_type {
                        PartitionType::Empty => {
                            debug!("PartitionIterator::next: got empty partition");
                            // looks like the extended partition is empty
                            (None, SetMbr::ToMbr(mbr))
                        }
                        PartitionType::Fat | PartitionType::Linux => {
                            debug!("PartitionIterator::next: got partition data partition");
                            self.index = 1;
                            self.part_idx += 1;

                            (
                                Some(PartInfo {
                                    index: self.part_idx,
                                    ptype: part.ptype,
                                    status: part.status,
                                    start_lba: self.offset + u64::from(part.first_lba),
                                    num_sectors: u64::from(part.num_sectors),
                                }),
                                SetMbr::ToMbr(mbr),
                            )
                        }
                        _ => {
                            error!(
                                "Unexpected partition type {:x} on index 0 of extended partition",
                                part.ptype
                            );
                            (None, SetMbr::Leave)
                        }
                    }
                }
                Err(why) => {
                    error!("Failed to read mbr, error:{:?}", why);
                    (None, SetMbr::Leave)
                }
            }
        };

        debug!(
            "PartitionIterator::next Res: {}",
            if res.is_some() { "some" } else { "none" }
        );

        match mbr {
            SetMbr::ToMbr(mbr) => {
                debug!("PartitionIterator::next set mbr");
                self.mbr = Some(mbr);
            }
            SetMbr::Leave => {
                debug!("PartitionIterator::next leave mbr");
            }
            SetMbr::ToNone => {
                debug!("PartitionIterator::next reset mbr");
                self.mbr = None;
            }
        }

        res
    }
}

pub(crate) struct PartitionReader<'a> {
    disk: &'a mut Disk,
    offset: u64,
    bytes_left: u64,
}

impl<'a> PartitionReader<'a> {
    pub fn from_part_iterator(
        part: &PartInfo,
        iterator: &'a mut PartitionIterator,
    ) -> PartitionReader<'a> {
        let block_size = iterator.disk.block_size;

        debug!(
            "from_part_iterator: start_lba: {}, num_sectors: {}, sector_size: {}",
            part.start_lba, part.num_sectors, block_size
        );

        PartitionReader {
            disk: iterator.disk,
            offset: part.start_lba * block_size,
            bytes_left: part.num_sectors * block_size,
        }
    }
}

impl<'a> Read for PartitionReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        if self.bytes_left == 0 {
            Ok(0)
        } else {
            let (res, size) = if self.bytes_left < buf.len() as u64 {
                (
                    self.disk
                        .disk
                        .fill(self.offset, &mut buf[0..self.bytes_left as usize]),
                    self.bytes_left as usize,
                )
            } else {
                (self.disk.disk.fill(self.offset, buf), buf.len())
            };

            match res {
                Ok(_) => {
                    self.offset += size as u64;
                    self.bytes_left -= size as u64;
                    Ok(size)
                }
                Err(why) => Err(Error::new(ErrorKind::UnexpectedEof, why.to_string())),
            }
        }
    }
}

/*
#[cfg(test)]
mod test {
    use crate::common::disk_util::PartitionIterator;
    use crate::common::disk_util::{Disk, LabelType};
<<<<<<< HEAD:src/migrator/linux/disk_util.rs
=======
    use crate::common::path_append;
    use std::path::{Path, PathBuf};

    fn get_test_file() -> PathBuf {
        let path_buf = PathBuf::from(file!());
        let mut test_path = path_buf.as_path();

        // iterate up the path to find project root
        let ancestors: Vec<&Path> = test_path.ancestors().collect();
        test_path = test_path.parent().unwrap();

        ancestors.iter().rev().find(|dir| {
            test_path = test_path.parent().unwrap();
            if &*dir.to_string_lossy() == "src" {
                true
            } else {
                false
            }
        });

        test_path = test_path.parent().unwrap();
        let test_file = path_append(path_append(&test_path, "test_data"), "part.img.gz");
        println!("using found test data path {}", test_file.display());
        test_file
    }
>>>>>>> consolidate:src/migrator/common/disk_util.rs

    #[test]
    fn read_gzipped_part() {
        let mut disk = Disk::from_gzip_img(get_test_file()).unwrap();
        if let LabelType::Dos = disk.get_label().unwrap() {
            let mut count = 0;
            let iterator = PartitionIterator::new(&mut disk).unwrap();
            for partition in iterator {
                match partition.index {
                    3 => assert_eq!(partition.ptype, 0x05),
                    _ => assert_eq!(partition.ptype, 0x83),
                }
                count += 1;
            }
            assert_eq!(count, 4);
        } else {
            panic!("Invalid label type - not Dos");
        }
    }
}
*/