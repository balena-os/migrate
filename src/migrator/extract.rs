use std::path::{Path, PathBuf};
use std::fs::{OpenOptions, remove_file, read_to_string};
use serde::Deserialize;
use log::{debug, info, error};
use std::io::{self, Read, Write};
use flate2::read::GzDecoder;
use serde::Deserialize;
use serde_json;


use crate::{
    common::{MigError, MigErrCtx, MigErrorKind,FileInfo, FileType, format_size_with_unit},
    linux::{
        ensured_cmds::{EnsuredCmds, MOUNT_CMD, SFDISK_CMD, MKTEMP_CMD, TRUNCATE_CMD, FILE_CMD}
    }
};

const REQUIRED_CMDS: &[&str] = &[MOUNT_CMD, SFDISK_CMD, MKTEMP_CMD, TRUNCATE_CMD, FILE_CMD];
const DEF_BLOCK_SIZE: u64 = 512;

const EXTRACT_FILE_TEMPLATE: &str = "extract.XXXXXXXXXX";

const BUFFER_SIZE: usize  = 1024 *1024; // 1Mb

/*
{
    "partitiontable": {
        "label": "dos",
        "id": "0x00000000",
        "device": "/dev/sda",
        "unit": "sectors",
        "partitions": [
            {"node": "/dev/sda1", "start": 8192, "size": 81920, "type": "e", "bootable": true},
            {"node": "/dev/sda2", "start": 90112, "size": 638976, "type": "83"},
            {"node": "/dev/sda3", "start": 729088, "size": 638976, "type": "83"},
            {"node": "/dev/sda4", "start": 1368064, "size": 15409152, "type": "f"},
            {"node": "/dev/sda5", "start": 1376256, "size": 40960, "type": "83"},
            {"node": "/dev/sda6", "start": 1425408, "size": 15351808, "type": "83"}
         ]
       }
}
*/

#[derive(Debug, Deserialize)]
pub(crate) struct SFDiskNode {
    start: u64,
    size: u64,
    #[serde(rename = "type")]
    ptype: String,
    bootable: Option<String>
}

#[derive(Debug, Deserialize)]
pub(crate) struct SFDiskPartTbl {
    label: String,
    partitions: Vev<SFDiskNode>
}


pub(crate) struct PartInfo {
    start_offset: u64,
    size: u64,
    ptype: String,
    bootable: bool,
    contents: PathBuf
}

pub(crate) struct ExtractInfo {
    part_label: String,
    block_size: u64,
    boot_part: PartInfo,
    roota_part: PartInfo,
    rootb_part: PartInfo,
    state_part: PartInfo,
    data_part: PartInfo,
}

pub(crate) struct Extractor {
    cmds: EnsuredCmds,
    image_path: PathBuf,
    gzipped: bool,
    json_supported: Option<bool>,
}

impl Extractor {
   pub fn new(image_path: &Path, work_dir: &Path) -> Result<Extractor, MigError> {
       let mut cmds = EnsuredCmds::new();
       if Err(why) = cmds.ensure_cmds(REQUIRED_CMDS) {
           error!("Some Required commands could not be found");
           return Err(MigError::displayed());
       }

       let image_info = FileInfo::new(image_path, work_dir)?;

       if let Some(image_info) = image_info {
           let gzipped = if image_info.is_type(&cmds, &FileType::GZipOSImage) {
               true
           } else {
               if image_info.is_type(&cmds, &FileType::OSImage) {
                   false
               } else {
                   error!("Found an unexpected file type in '{}', not an OS image");
                   return Err(MigError::displayed());
               }
           };

           Ok(Extractor{
               cmds,
               image_path: image_info.path,
               gzipped,
               json_supported: None,
           })
       } else {
           error!("The image file could not be found: '{}'", image_path.display());
           Err(MigError::displayed()
       }
   }


   pub fn extract(&mut self) -> Result<ExtractInfo, MigError> {

       if self.gzipped {
           // create temp_file
           let cmd_res = cmds.call(MKTEMP_CMD, &["-p", &work_dir.to_string_lossy(), EXTRACT_FILE_TEMPLATE ], true)?;
           let tmp_file = if cmd_res.status.success() {
               PathBuf::from(cmd_res.stdout)
           } else {
               error!("Failed to create a temporary file in '{}'", work_dir.display());
               return Err(MigError::displayed());
           };
       }

       unimplemented!()
   }

    fn get_gzip_part_info(&mut self, extract_tmp: &Path) -> Result<PartInfo, MigError> {
        const READ_FIRST_ATTMPT: u64 = 1024 * 1024;
        let bytes_written = self.gzip_extract(
            extract_tmp,
            0,
            READ_FIRST_ATTMPT / DEF_BLOCK_SIZE,
            DEF_BLOCK_SIZE)?;

        if bytes_written <  READ_FIRST_ATTMPT {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format("Failed to read {} from gzipped image: '{}'", format_size_with_unit(READ_FIRST_ATTMPT), self.image_path.display())))
        }

        let part_tbl = self.get_part_info(extract_tmp)?;

        if (part_tbl.partitions.len() == 4)  && (part_tbl.partitions[3].ptype == "f") {
            let ext_start = part_tbl.partitions[3].start;
            // expected incomplete partition table - extract offset of 3rd / extended partition and retry

            let bytes_written = self.gzip_extract(
                extract_tmp,
                0,
                ext_start,
                DEF_BLOCK_SIZE)?;
            if bytes_written < (ext_start * DEF_BLOCK_SIZE) {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format("Failed to read {} from gzipped image: '{}'", format_size_with_unit(ext_start * DEF_BLOCK_SIZE), self.image_path.display())))
            }

            let part_tbl = self.get_part_info(extract_tmp)?;

            if (part_tbl.partitions.len() != 6) {
                error!("Encountered unexpected partition table in '{}', expected 6 parrtitions, got {}", self.image_path.display(), part_tbl.partitions.len());
                return Err(MigError::displayed());
            }
        }

        unimplemented!()
    }




    fn get_part_info(&mut self, image_path: &Path) -> Result<SFDiskPartTbl, MigError> {
        let json_supported = if let Some(json_supported) = self.json_supported {
            json_supported
        } else {
            true;
        };

        if json_supported {
            // try the new format
            match self.cmds.call(SFDISK_CMD, &["--json", &image_path.to_string_lossy()], true) {
                Ok(cmd_res) => {
                    if cmd_res.status.success() {
                        self.json_supported = Some(true);
                        match serde_json::from_str::<SFDiskPartTbl>(&cmd_res.stdout) {
                            Ok(part_info) => {
                                return Ok(part_info);
                            }
                            Err(why) => {
                                error!("Failed to deserialize {} output from json, error: {:?}", SFDISK_CMD, why);
                                return Err(MigError::displayed());
                            }
                        }
                    } else {
                        self.json_supported = Some(false);
                    }
                },
                Err(why) => {
                    error!("Failed to call {} on '{}'", error: {:?}, SFDISK_CMD, image_path.display(), why);
                    return Err(MigError::displayed());
                }
            }
        }

        // TODO: parse old format output to part_info
        unimplemented!()

        /*match self.cmds.call(SFDISK_CMD, &["--dump", &image_path.to_string_lossy()], true) {
        }
        */
    }



    fn gzip_extract(&self, dest_path: &Path, start_block: u64, block_count: u64, block_size: u64) -> Result<u64, MigError> {
        debug!("creating output file: '{}'", dest_path.display());

        let mut dest_file = match OpenOptions::new()
            .write(true)
            .read(false)
            .create(true)
            .truncate(true)
            .open(dest_path) {
            Ok(file) => file,
            Err(why) => {
                error!("failed to open output file for writing: '{}', error {:?}", dest_path.display(), why);
                return Err(MigError::displayed());
            }
        };

        let image_file = match OpenOptions::new()
            .write(false)
            .read(true)
            .create(false)
            .open(image_path) {
            Ok(file) => file,
            Err(why) => {
                error!("failed to open image file for reading: '{}', error {:?}", image_path.display(), why);
                return Err(MigError::displayed());
            }
        };

        let mut decoder = GzDecoder::new(image_file);
        let max = block_count * block_size;

        let buffer: &mut [u8] = &mut [0; BUFFER_SIZE];
        let mut bytes_written: u64 = 0;

        loop {
            let bytes_read = match decoder.read(buffer) {
                Ok(bytes_read) => bytes_read,
                Err(why) => {
                    error!("Failed to read from input '{}', error: {:?}", image_path.display(), why);
                    return Err(MigError::displayed());
                }
            };

            if bytes_read == 0 {
                return Ok(bytes_written);
            }

            let written = match dest_file.write(&buffer[0..bytes_read]) {
                Ok(written) => written,
                Err(why) => {
                    error!("Failed to write to output '{}', error: {:?}", dest_file.display(), why);
                    return Err(MigError::displayed());
                }
            };

            if written != bytes_read {
                error!("Differing values of bytes written & bytes read {} != {}", written, bytes_read);
            }

            bytes_written += written as u64;

            if bytes_written >= max {
                return  Ok(bytes_written);
            }
        }
    }


fn get_part_info_json(cmds: &EnsuredCmds, image_file: &FileInfo) -> Result<PartInfo, MigError> {
    unimplemented!()


}

fn get_part_info_text(cmds: &EnsuredCmds, image_file: &Path) -> Result<PartInfo, MigError> {

    cmds.call(SFDISK_CMD, &[])


}


}