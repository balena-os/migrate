// TODO: flash image using DD

use flate2::read::GzDecoder;
use log::{debug, error, info};
use mod_logger::Logger;
use nix::unistd::sync;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::{
    common::{format_size_with_unit, stage2_config::Stage2Config},
    linux::ensured_cmds::{EnsuredCmds, DD_CMD, GZIP_CMD},
};

const DD_BLOCK_SIZE: usize = 4194304;

// TODO: partition & untar balena to drive

// TODO: return something else instead (success, (recoverable / not recoverable))

pub(crate) enum FlashResult {
    Ok,
    FailRecoverable,
    FailNonRecoverable,
}

fn flash_gzip_internal(
    dd_cmd: &str,
    target_path: &Path,
    // cmds: &EnsuredCmds,
    image_path: &Path,
) -> FlashResult {
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
    /*    {
            Ok(decoder) => decoder,
            Err(why) => {
                error!("Failed to create gzip decoder from image file '{}', error: {:?}", image_path.display(), why);
                return FlashResult::FailRecoverable;
            }
        };
    */
    debug!("invoking dd");

    let mut dd_child = match Command::new(dd_cmd)
        .args(&[
            // "conv=fsync", sadly not supported on busybox dd
            // "oflag=direct",
            &format!("of={}", &target_path.to_string_lossy()),
            &format!("bs={}", DD_BLOCK_SIZE),
        ])
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(dd_child) => dd_child,
        Err(why) => {
            error!("failed to execute command {}, error: {:?}", dd_cmd, why);
            return FlashResult::FailRecoverable;
        }
    };

    Logger::flush();

    let start_time = Instant::now();
    let mut last_elapsed = Duration::new(0, 0);
    let mut write_count: usize = 0;

    let mut fail_res = FlashResult::FailRecoverable;
    if let Some(ref mut stdin) = dd_child.stdin {
        let mut buffer: [u8; DD_BLOCK_SIZE] = [0; DD_BLOCK_SIZE];
        loop {
            let bytes_read = match decoder.read(&mut buffer) {
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
                fail_res = FlashResult::FailNonRecoverable;

                let bytes_written = match stdin.write(&buffer[0..bytes_read]) {
                    Ok(bytes_written) => bytes_written,
                    Err(why) => {
                        error!("Failed to write uncopressed data to dd, error {:?}", why);
                        return fail_res;
                    }
                };

                write_count += bytes_written;

                if bytes_read != bytes_written {
                    error!(
                        "Read/write count mismatch, read {}, wrote {}",
                        bytes_read, bytes_written
                    );
                    Logger::flush();
                }

                let curr_elapsed = start_time.elapsed();
                let since_last = match curr_elapsed.checked_sub(last_elapsed) {
                    Some(dur) => dur,
                    None => Duration::from_secs(0)
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
            } else {
                break;
            }
        }

        match dd_child.wait_with_output() {
            Ok(_) => (),
            Err(why) => {
                error!("Error while waiting for dd to terminate:{:?}", why);
                Logger::flush();
                return fail_res;
            }
        }

        let secs_elapsed = start_time.elapsed().as_secs();
        info!(
            "{} written @ {}/sec in {} seconds",
            format_size_with_unit(write_count as u64),
            format_size_with_unit(write_count as u64 / secs_elapsed),
            secs_elapsed
        );
        Logger::flush();

        FlashResult::Ok
    } else {
        error!("Failed to get a stdin for dd");
        Logger::flush();
        FlashResult::FailRecoverable
    }
}

fn flash_gzip_external(
    dd_cmd: &str,
    target_path: &Path,
    cmds: &EnsuredCmds,
    image_path: &Path,
) -> FlashResult {
    if let Ok(ref gzip_cmd) = cmds.get(GZIP_CMD) {
        debug!("gzip found at: {}", gzip_cmd);
        let gzip_child = match Command::new(gzip_cmd)
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
                    if dd_cmd_res.status.success() == true {
                        return FlashResult::Ok;
                    } else {
                        error!(
                            "dd terminated with exit code: {:?}",
                            dd_cmd_res.status.code()
                        );
                        return FlashResult::FailNonRecoverable;
                    }
                }
                Err(why) => {
                    error!("failed to execute command {}, error: {:?}", dd_cmd, why);
                    return FlashResult::FailRecoverable;
                }
            }
        } else {
            error!("failed to retrieved gzip stdout)");
            return FlashResult::FailRecoverable;
        }
    } else {
        error!("{} command was not found, cannot flash image", GZIP_CMD);
        return FlashResult::FailRecoverable;
    }
}

pub(crate) fn flash(
    target_path: &Path,
    cmds: &EnsuredCmds,
    config: &Stage2Config,
    image_path: &Path,
) -> FlashResult {
    if let Ok(ref dd_cmd) = cmds.get(DD_CMD) {
        debug!("dd found at: {}", dd_cmd);
        let res = if config.is_gzip_internal() {
            flash_gzip_internal(dd_cmd, target_path, image_path)
        } else {
            flash_gzip_external(dd_cmd, target_path, cmds, image_path)
        };

        sync();

        res
    } else {
        error!("{} command was not found, cannot flash image", DD_CMD);
        FlashResult::FailRecoverable
    }
}
