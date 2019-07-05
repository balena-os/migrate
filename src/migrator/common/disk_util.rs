use failure::ResultExt;
use std::path::{Path};
use std::fs::{OpenOptions};
use std::io::{Read,Seek, SeekFrom};
use std::mem;
use log::{error, warn, debug, trace};

use crate::{
    common::{
        MigError, MigErrCtx, MigErrorKind, },
};


mod image_file;
pub(crate) use image_file::ImageFile;

mod gzip_file;
pub(crate) use gzip_file::GZipFile;

mod plain_file;
pub(crate) use plain_file::PlainFile;

const DEF_BLOCK_SIZE: usize = 512;

// TODO: create test with gzipped partition file

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
    fill0: [u8; 446],
    part_tbl: [PartEntry; 4],
    boot_sig1: u8,
    boot_sig2: u8,
}

pub(crate) struct Partition {
    pub index: usize,
    pub ptype: u8,
    pub status: u8,
    pub start_lba: u64,
    pub num_sectors: u64,
}



#[derive(Debug)]
pub(crate) enum LabelType {
    GPT,
    DOS,
}


pub(crate) struct Disk {
    disk: Box<ImageFile>,
    writable: bool,
    block_size: u64,
}

impl Disk {
    pub fn from_gzip_img<P: AsRef<Path>>(image: P) -> Result<Disk, MigError> {
        Ok(Disk{
            disk: Box::new(GZipFile::new(image.as_ref())?),
            writable: false,
            block_size: DEF_BLOCK_SIZE as u64,
        })
    }

    pub fn from_drive_file<P: AsRef<Path>>(drive: P, writable: bool, block_size: Option<u64>) -> Result<Disk, MigError> {
        Ok(Disk{
            disk: Box::new(PlainFile::new(drive.as_ref())?),
            writable,
            block_size: if let Some(block_size) = block_size { block_size } else { DEF_BLOCK_SIZE as u64 },
        })
    }

    pub fn get_label(&mut self) -> Result<LabelType, MigError> {
        let mbr = self.read_mbr(0)?;
        let p0type = mbr.part_tbl[0].ptype;
        if p0type == 0xEE {
            Ok(LabelType::GPT)
        } else {
            Ok(LabelType::DOS)
        }
    }

    fn read_mbr(&mut self, block_idx: u64) -> Result<MasterBootRecord, MigError> {
        let mut buffer: [u8; DEF_BLOCK_SIZE] = [0; DEF_BLOCK_SIZE];

        self.disk.fill(block_idx * DEF_BLOCK_SIZE as u64, &mut buffer)?;

        let mbr: MasterBootRecord = unsafe { mem::transmute(buffer) };

        if (mbr.boot_sig1 != 0x55) || (mbr.boot_sig2 != 0xAA) {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("Encountered an invalid MBR signature, expected 0x55, 0xAA,  got {:x}, {:x}",
                         mbr.boot_sig1, mbr.boot_sig2)))
        }

        Ok(mbr)
    }

    pub fn get_partition_iterator(&mut self) -> Result<PartitionIterator, MigError> {
        Ok(PartitionIterator::new(self)?)
    }
}

pub(crate) struct PartitionIterator<'a> {
    disk: &'a mut Disk,
    mbr: Option<MasterBootRecord>,
    offset: u64,
    index: usize,
    part_idx: usize,
}

impl<'a> PartitionIterator<'a> {
    pub fn new(disk: &mut Disk) -> Result<PartitionIterator, MigError> {
        let offset: u64 = 0;
        let mbr = disk.read_mbr(offset)?;

        Ok(PartitionIterator {
            disk,
            mbr: Some(mbr),
            offset,
            index: 0,
            part_idx: 0,
        })
    }
}


// TODO: make functions for partition type:
// is extended
// is None
// is regular

impl<'a> Iterator for PartitionIterator<'a> {
    type Item = Partition;

    fn next(&mut self) -> Option<Self::Item> {
        trace!("PartitionIterator::next: entered" );
        // TODO: check for 0 size partition ?

        enum SetMbr {
            Leave,
            ToNone,
            ToMbr(MasterBootRecord)
        }


        let (res, mbr) = if let Some(ref mbr) = self.mbr {
            if self.offset == 0 {
                debug!("PartitionIterator::next: offset: {}, index: {}, part_idx: {}, mbr: present",
                       self.offset, self.index, self.part_idx);
                // we are on the first partition table
                if self.index > 3 {
                    // end of regular partition table reached
                    (None, SetMbr::Leave)
                } else {
                    // read regular partition
                    let part = &mbr.part_tbl[self.index];
                    match part.ptype {
                        0x00 =>
                        // empty partition - Assume End of Table
                            (None, SetMbr::Leave),
                        0x05 | 0x0F => { // extended / container
                            // return extended partition
                            self.offset = part.first_lba as u64;
                            // self.mbr = None; // we are done with this mbr
                            self.part_idx += 1;
                            (Some(Partition {
                                index: self.part_idx,
                                ptype: part.ptype,
                                status: part.status,
                                start_lba: part.first_lba as u64,
                                num_sectors: part.num_sectors as u64,
                            }), SetMbr::ToNone)
                        },
                        _ => { // return regular partition
                            self.index += 1;
                            self.part_idx += 1;
                            (Some(Partition {
                                index: self.part_idx,
                                ptype: part.ptype,
                                status: part.status,
                                start_lba: part.first_lba as u64,
                                num_sectors: part.num_sectors as u64,
                            }), SetMbr::Leave)
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
                    match part.ptype {
                        0x00 => {
                            // regular end  of extended partitions
                            // // warn!("Empty partition on index 1 of extended partition is unexpected");
                            (None, SetMbr::Leave)
                        }, // weird though
                        0x05 | 0x0F => { // we are expecting a regular partition here
                            self.offset += part.first_lba as u64;
                            match self.disk.read_mbr(self.offset) {
                                Ok(mbr) => {
                                    let part = &mbr.part_tbl[0];
                                    // self.mbr = Some(mbr);
                                    self.index = 1;
                                    self.part_idx += 1;
                                    (Some(Partition {
                                        index: self.part_idx,
                                        ptype: part.ptype,
                                        status: part.status,
                                        start_lba: self.offset + part.first_lba as u64,
                                        num_sectors: part.num_sectors as u64,
                                    }), SetMbr::ToMbr(mbr))
                                },
                                Err(why) => {
                                    error!("Failed to read mbr, error:{:?}", why);
                                    (None, SetMbr::Leave)
                                }
                            }
                        },
                        _ => {
                            error!("Unexpected partition type {:x} on index 1 of extended partition", part.ptype);
                            (None, SetMbr::Leave)
                        }
                    }
                }
            }
        } else {
            // this only happens after the first extended partition has been reported
            debug!("PartitionIterator::next: offset: {}, index: {}, part_idx: {}, mbr: absent",
                    self.offset, self.index, self.part_idx);
            match self.disk.read_mbr(self.offset) {
                Ok(mbr) => {
                    let part = &mbr.part_tbl[self.index];
                    // self.mbr = Some(mbr);
                    if part.ptype == 0x00 {
                        // looks like the extended partition is empty
                        (None, SetMbr::ToMbr(mbr))
                    } else {
                        self.index = 1;
                        self.part_idx += 1;
                        (Some(Partition {
                            index: self.part_idx,
                            ptype: part.ptype,
                            status: part.status,
                            start_lba: self.offset + part.first_lba as u64,
                            num_sectors: part.num_sectors as u64,
                        }), SetMbr::ToMbr(mbr))
                    }
                },
                Err(why) => {
                    error!("Failed to read mbr, error:{:?}", why);
                    (None, SetMbr::Leave)
                }
            }
        };

        match mbr {
            SetMbr::ToMbr(mbr) => {
                self.mbr = Some(mbr);
            },
            SetMbr::Leave => (),
            SetMbr::ToNone => {
                self.mbr = None;
            },
        }

        res
    }
}


