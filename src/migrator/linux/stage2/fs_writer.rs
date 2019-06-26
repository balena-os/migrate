use std::path::Path;
use std::process::{Command, Stdio};
use failure::{ResultExt};

use crate::{
    common::{stage2_config::Stage2Config, MigError, MigErrCtx, MigErrorKind},
    linux::{ extract::{Partition},
             ensured_cmds::{EnsuredCmds, SFDISK_CMD},
    },
};
use std::io::Write;

// TODO: partition & untar balena to drive

pub(crate) fn partition(device: &Path, cmds: &EnsuredCmds, config: &Stage2Config, partitions: &Vec<Partition>) -> Result<(), MigError> {
    let sfdisk_path = cmds.get(SFDISK_CMD)?;

    let sfdisk_cmd = Command::new(sfdisk_path)
        .args(&[&device.to_string_lossy()])
        .stderr(Stdio::piped())
        .stdin(Stdio::piped())
        .spawn()
        .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed to start command : '{}'", sfdisk_path)))?;

    {
        let stdin = sfdisk_cmd.stdin;
        if let Some(ref mut stdin) = stdin {
            let _res = stdin.write("label: dos".as_bytes())
                .context(MigErrCtx::from_remark(MigErrorKind::Upstream, "Failed to write to sfdisk stdin"))?;

            let mut part_idx = 0;
            for curr_part in &partitions {

                if part_idx == 3 {
                    let _res = stdin.write("type=5".as_bytes())
                    .context(MigErrCtx::from_remark(MigErrorKind::Upstream, "Failed to write to sfdisk stdin"))?;
                }

                part_idx += 1;

                let bootable = if (curr_part.status & 0x80) == 0x80 {
                    "bootable,"
                } else {
                    ""
                };

                let _res = stdin.write(
                    &format!("start={},size={},bootable={},type={:x}",
                             curr_part.start_lba,
                             curr_part.num_sectors,
                    ,
                    curr_part.ptype
                ).as_bytes())
                .context(MigErrCtx::from_remark(MigErrorKind::Upstream, "Failed to write to sfdisk stdin"))?;

                let curr_part = &partitions[0];
                let _res = stdin.write(
                    &format!("start={},size={},{}type={:x}",
                             curr_part.start_lba,
                             curr_part.num_sectors,
                             bootable,
                             curr_part.ptype
                    ).as_bytes())
                    .context(MigErrCtx::from_remark(MigErrorKind::Upstream, "Failed to write to sfdisk stdin"))?;
            }
        }
    }

    let cmd_res = sfdisk_cmd.wait_with_output()
        .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("failure waiting for sfdisk to terminate")))?;



    unimplemented!()
}
