// TODO: flash image using DD

use flate2::read::GzDecoder;
use log::{debug, error, info};
use mod_logger::Logger;
use nix::unistd::sync;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::str;
use std::thread;
use std::time::{Duration, Instant};

use crate::{
    common::{call, format_size_with_unit, stage2_config::Stage2Config},
    linux::{
        linux_defs::{DD_CMD, GZIP_CMD, PARTPROBE_CMD, UDEVADM_CMD},
        linux_defs::{POST_PARTPROBE_WAIT_SECS, PRE_PARTPROBE_WAIT_SECS},
        stage2::{mounts::Mounts, FlashResult},
    },
};

// TODO: minimum recommended size 128K
const DD_BLOCK_SIZE: usize = 128 * 1024; // 4_194_304;
const UDEVADM_PARAMS: &[&str] = &["settle", "-t", "10"];

// TODO: replace removed command checks ?
//const REQUIRED_CMDS: &[&str] = &[DD_CMD, PARTPROBE_CMD, UDEVADM_CMD];

// TODO: return something else instead (success, (recoverable / not recoverable))

pub(crate) fn flash_balena_os(
    target_path: &Path,
    mounts: &mut Mounts,
    config: &Stage2Config,
    image_path: &Path,
) -> FlashResult {
    let res = if config.is_gzip_internal() {
        flash_gzip_internal(DD_CMD, target_path, image_path)
    } else {
        flash_gzip_external(DD_CMD, target_path, image_path)
    };

    sync();

    info!(
        "The Balena OS image has been written to the device '{}'",
        target_path.display()
    );

    thread::sleep(Duration::from_secs(PRE_PARTPROBE_WAIT_SECS));

    let _res = call(PARTPROBE_CMD, &[&target_path.to_string_lossy()], true);

    thread::sleep(Duration::from_secs(POST_PARTPROBE_WAIT_SECS));

    let _res = call(UDEVADM_CMD, UDEVADM_PARAMS, true);

    if let Err(why) = mounts.mount_balena(false) {
        error!("Failed to mount balena partitions, error: {:?}", why);
        return FlashResult::FailNonRecoverable;
    }

    res
}

fn flash_gzip_internal(_dd_cmd: &str, target_path: &Path, image_path: &Path) -> FlashResult {
    debug!("opening: '{}'", image_path.display());

    let mut decoder = GzDecoder::new(match File::open(&image_path) {
        Ok(file) => file,
        Err(why) => {
            error!(
                "Failed to open image file '{}', error: {:?}",
                image_path.display(),
                why
            );
            return FlashResult::FailRecoverable;
        }
    });

    /* debug!("invoking dd");

    let mut dd_child = match Command::new(dd_cmd)
        .args(&[
            // "conv=fsync", sadly not supported on busybox dd
            // "oflag=direct",
            &format!("of={}", &target_path.to_string_lossy()),
            &format!("bs={}", DD_BLOCK_SIZE),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit()) // test
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(dd_child) => dd_child,
        Err(why) => {
            error!("failed to execute command {}, error: {:?}", dd_cmd, why);
            Logger::flush();
            return FlashResult::FailRecoverable;
        }
    };
    */

    debug!("opening output file '{}", target_path.display());
    let mut out_file = match OpenOptions::new()
        .write(true)
        .read(false)
        .create(false)
        .open(&target_path)
    {
        Ok(file) => file,
        Err(why) => {
            error!(
                "Failed to open output file '{}', error: {:?}",
                target_path.display(),
                why
            );
            return FlashResult::FailRecoverable;
        }
    };

    let start_time = Instant::now();
    let mut last_elapsed = Duration::new(0, 0);
    let mut write_count: usize = 0;

    let mut fail_res = FlashResult::FailRecoverable;
    // TODO: might pay to put buffer on page boundary
    let mut buffer: [u8; DD_BLOCK_SIZE] = [0; DD_BLOCK_SIZE];
    loop {
        // fill buffer
        let mut buff_fill: usize = 0;
        loop {
            let bytes_read = match decoder.read(&mut buffer[buff_fill..]) {
                Ok(bytes_read) => bytes_read,
                Err(why) => {
                    error!(
                        "Failed to read uncompressed data from '{}', error: {:?}",
                        image_path.display(),
                        why
                    );
                    return fail_res;
                }
            };

            if bytes_read > 0 {
                buff_fill += bytes_read;
                if buff_fill < buffer.len() {
                    continue;
                }
            }
            break;
        }

        if buff_fill > 0 {
            fail_res = FlashResult::FailNonRecoverable;

            let bytes_written = match out_file.write(&buffer[0..buff_fill]) {
                Ok(bytes_written) => bytes_written,
                Err(why) => {
                    error!("Failed to write uncompressed data to dd, error {:?}", why);
                    return fail_res;
                }
            };

            write_count += bytes_written;

            if buff_fill != bytes_written {
                error!(
                    "Read/write count mismatch, read {}, wrote {}",
                    buff_fill, bytes_written
                );
                return fail_res;
            }

            let curr_elapsed = start_time.elapsed();
            let since_last = match curr_elapsed.checked_sub(last_elapsed) {
                Some(dur) => dur,
                None => Duration::from_secs(0),
            };

            if since_last.as_secs() >= 10 {
                last_elapsed = curr_elapsed;
                let secs_elapsed = curr_elapsed.as_secs();
                info!(
                    "{} written @ {}/sec in {} seconds",
                    format_size_with_unit(write_count as u64),
                    format_size_with_unit(write_count as u64 / secs_elapsed),
                    secs_elapsed
                );
                Logger::flush();
            }

            if buff_fill < buffer.len() {
                break;
            }
        } else {
            break;
        }
    }

    /*
    match dd_child.wait_with_output() {
        Ok(cmd_res) => {
            if !cmd_res.status.success() {
                let stderr = match str::from_utf8(&cmd_res.stderr) {
                    Ok(stderr) => stderr,
                    Err(_) => "- invalid utf8 -",
                };
                error!(
                    "dd reported an error: code: {:?}, stderr: {}",
                    cmd_res.status.code(),
                    stderr
                );
                // might pay to still try and finish as all input was written
            }
        }
        Err(why) => {
            error!("Error while waiting for dd to terminate:{:?}", why);
            Logger::flush();
            return fail_res;
        }
    }
    */

    let secs_elapsed = start_time.elapsed().as_secs();
    info!(
        "{} written @ {}/sec in {} seconds",
        format_size_with_unit(write_count as u64),
        format_size_with_unit(write_count as u64 / secs_elapsed),
        secs_elapsed
    );

    FlashResult::Ok
}

fn flash_gzip_external(dd_cmd: &str, target_path: &Path, image_path: &Path) -> FlashResult {
    let gzip_child = match Command::new(GZIP_CMD)
        .args(&["-d", "-c", &image_path.to_string_lossy()])
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(gzip_child) => gzip_child,
        Err(why) => {
            error!("Failed to create gzip process, error: {:?}", why);
            return FlashResult::FailRecoverable;
        }
    };

    if let Some(stdout) = gzip_child.stdout {
        debug!("invoking dd");
        match Command::new(dd_cmd)
            .args(&[
                &format!("of={}", &target_path.to_string_lossy()),
                &format!("bs={}", DD_BLOCK_SIZE),
            ])
            .stdin(stdout)
            .output()
        {
            Ok(dd_cmd_res) => {
                if dd_cmd_res.status.success() {
                    FlashResult::Ok
                } else {
                    error!(
                        "dd terminated with exit code: {:?}",
                        dd_cmd_res.status.code()
                    );
                    FlashResult::FailNonRecoverable
                }
            }
            Err(why) => {
                error!("failed to execute command {}, error: {:?}", dd_cmd, why);
                FlashResult::FailRecoverable
            }
        }
    } else {
        error!("failed to retrieved gzip stdout)");
        FlashResult::FailRecoverable
    }
}
