use libc::c_int;
use log::{debug, error, info, warn};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::mem;
use std::os::unix::io::AsRawFd;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::common::config::migrate_config::WatchdogCfg;
use crate::common::MigError;

const WD_IOC_MAGIC: u8 = b'W';

// const WD_IOC_GETSTATUS: u8 = 1;
// const WD_IOC_SETTIMEOUT: u8 = 6;

const WD_IOC_GETSUPPORT: u8 = 0;
const WD_IOC_KEEPALIVE: u8 = 5;
const WD_IOC_GETTIMEOUT: u8 = 7;
const WD_IOC_GETTIMELEFT: u8 = 10;

const WDIOF_MAGICCLOSE: u32 = 0x0100; /* Supports magic close char */

const SECOND_2_NANO: u32 = 1000000000;

#[repr(C)]
#[derive(Clone)]
struct WatchdogInfo {
    options: u32,          /* Options the card/driver supports */
    firmware_version: u32, /* Firmware version of the card */
    identity: [u8; 32],    /* Identity of the board */
}

ioctl_read!(wdioc_get_timeout, WD_IOC_MAGIC, WD_IOC_GETTIMEOUT, c_int);
ioctl_read!(wdioc_get_timeleft, WD_IOC_MAGIC, WD_IOC_GETTIMELEFT, c_int);
ioctl_read!(wdioc_keepalive, WD_IOC_MAGIC, WD_IOC_KEEPALIVE, c_int);
ioctl_read_buf!(wdioc_get_support, WD_IOC_MAGIC, WD_IOC_GETSUPPORT, u8);

// ioctl_read!(wdioc_get_status, WD_IOC_MAGIC, WD_IOC_GETSTATUS, c_int);
// ioctl_write_ptr!(wdioc_set_timeout, WD_IOC_MAGIC, WD_IOC_SETTIMEOUT, c_int);

struct Watchdog {
    config: WatchdogCfg,
    file: Option<File>,
    fd: c_int,
    info: Option<WatchdogInfo>,
    interval: u64,
    due: Duration,
    kicked: Instant,
}

impl Watchdog {
    pub fn new(watchdog_cfg: &WatchdogCfg) -> Result<Watchdog, MigError> {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create(false)
            .open(&*watchdog_cfg.path.to_string_lossy())
        {
            Ok(file) => {
                let fd = file.as_raw_fd();
                let mut watchdog_buff: [u8; mem::size_of::<WatchdogInfo>()] =
                    [0; mem::size_of::<WatchdogInfo>()];

                let info = match unsafe { wdioc_get_support(fd, &mut watchdog_buff) } {
                    Ok(_) => {
                        let watchdog_info: WatchdogInfo = unsafe { mem::transmute(watchdog_buff) };
                        debug!(
                            "io_ctl result for get_support on '{}' retured Ok",
                            watchdog_cfg.path.display()
                        );
                        debug!(
                            "io_ctl result for get_support on '{}' retured options: 0x{:x}",
                            watchdog_cfg.path.display(),
                            watchdog_info.options
                        );
                        Some(watchdog_info.clone())
                    }
                    Err(why) => {
                        warn!(
                            "io_ctl result for get_support on '{}' failed {:?}",
                            watchdog_cfg.path.display(),
                            why
                        );
                        None
                    }
                };

                let interval = if let Some(interval) = watchdog_cfg.interval {
                    interval
                } else {
                    let mut interval: c_int = 0;
                    match unsafe { wdioc_get_timeout(fd, &mut interval) } {
                        Ok(_) => interval as u64,
                        Err(why) => {
                            warn!(
                                "Failed to retrieve timeout from watchdog: '{}', error: {:?}",
                                watchdog_cfg.path.display(),
                                why
                            );
                            60
                        }
                    }
                };

                let mut due: c_int = 0;
                match unsafe { wdioc_get_timeleft(fd, &mut due) } {
                    Ok(_) => (),
                    Err(why) => {
                        // TODO: assume 60 ?
                        warn!(
                            "Failed to retrieve remaining time from watchdog: '{}', error: {:?}",
                            watchdog_cfg.path.display(),
                            why
                        );
                        due = 1;
                    }
                }

                Ok(Watchdog {
                    config: watchdog_cfg.clone(),
                    file: Some(file),
                    fd,
                    info,
                    kicked: Instant::now(),
                    interval,
                    due: Duration::new((due - 1) as u64, SECOND_2_NANO / 2),
                })
            }
            Err(why) => {
                warn!(
                    "Failed to open watchdog '{}', error: {:?}",
                    watchdog_cfg.path.display(),
                    why
                );
                Err(MigError::displayed())
            }
        }
    }

    pub fn is_close(&self) -> bool {
        if let Some(fl_close) = self.config.close {
            fl_close
        } else {
            true
        }
    }

    pub fn has_magic_close(&self) -> bool {
        if let Some(ref wd_info) = self.info {
            (wd_info.options & WDIOF_MAGICCLOSE) == WDIOF_MAGICCLOSE
        } else {
            false
        }
    }

    pub fn kick(&mut self) {
        let mut status: c_int = 0;
        if let Err(why) = unsafe { wdioc_keepalive(self.fd, &mut status) } {
            error!(
                "wdioc_keepalive '{}', failed with: {:?}",
                self.config.path.display(),
                why
            );
        } else {
            debug!(
                "wdioc_keepalive '{}': 0x{:x}",
                self.config.path.display(),
                status
            );
        }
        self.kicked = Instant::now();
        self.due = Duration::new(self.interval - 1, SECOND_2_NANO / 2);
    }

    pub fn kick_if_due(&mut self) {
        if self.kicked.elapsed() >= self.due {
            self.kick();
        }
    }

    pub fn close(&mut self) {
        if self.has_magic_close() {
            if let Some(ref mut file) = self.file {
                let buf: [u8; 1] = [b'V'];
                match file.write(&buf) {
                    Ok(_) => (),
                    Err(why) => {
                        error!(
                            "Failed to write close byte to '{}', error {:?}",
                            self.config.path.display(),
                            why
                        );
                    }
                }
            }
        }
        self.file = None;
    }
}

pub(crate) struct WatchdogHandler {
    tx: Option<Sender<usize>>,
    join_handle: Option<JoinHandle<()>>,
}

impl WatchdogHandler {
    pub fn new(watchdogs: &Vec<WatchdogCfg>) -> Result<WatchdogHandler, MigError> {
        let mut dogs: Vec<Watchdog> = Vec::new();

        for watchdog_cfg in watchdogs {
            match Watchdog::new(watchdog_cfg) {
                Ok(mut watchdog) => {
                    if watchdog.is_close() && watchdog.has_magic_close() {
                        watchdog.kick();
                        watchdog.close();
                        debug!(
                            "created and closed watchdog for: '{}'",
                            watchdog_cfg.path.display()
                        );
                        continue;
                    } else {
                        debug!("created watchdog for: '{}'", watchdog_cfg.path.display());
                        dogs.push(watchdog);
                    }
                }
                Err(why) => {
                    warn!(
                        "Failed to initialize watchdog: '{}': error {:?}",
                        watchdog_cfg.path.display(),
                        why
                    );
                }
            }
        }

        if !dogs.is_empty() {
            let (tx, rx) = mpsc::channel::<usize>();
            let join_handle = thread::spawn(move || {
                WatchdogHandler::wd_thread(rx, dogs);
            });

            Ok(WatchdogHandler {
                tx: Some(tx),
                join_handle: Some(join_handle),
            })
        } else {
            info!("No watchdogs remain to kick");
            Ok(WatchdogHandler {
                tx: None,
                join_handle: None,
            })
        }
    }

    pub fn stop(&mut self) {
        if let Some(ref tx) = self.tx {
            let _res = tx.send(1);

            let mut local_handle: Option<JoinHandle<()>> = None;
            mem::swap(&mut local_handle, &mut self.join_handle);

            if let Some(join_handle) = local_handle {
                let _res = join_handle.join();
            }
        }
        self.tx = None;
    }

    fn wd_thread(rx: Receiver<usize>, mut dogs: Vec<Watchdog>) {
        loop {
            thread::sleep(Duration::new(0, SECOND_2_NANO / 2));
            if let Ok(_msg) = rx.try_recv() {
                info!("Watchdog thread is terminating");
                break;
            }

            for watchdog in dogs.iter_mut() {
                watchdog.kick_if_due();
            }
        }

        loop {
            if let Some(mut current) = dogs.pop() {
                current.kick();
                current.close();
            } else {
                break;
            }
        }
    }
}
