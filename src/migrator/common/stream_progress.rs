use log::{debug, error, info, trace, warn, Level};
use std::io::Read;
use std::time::Instant;

use crate::common::format_size_with_unit;

pub(crate) struct StreamProgress<T> {
    input: T,
    bytes_read: u64,
    last_log: u64,
    every: u32,
    level: Level,
    start_time: Instant,
}

impl<T: Read> StreamProgress<T> {
    pub fn new(input: T, every: u32, level: Level) -> StreamProgress<T> {
        StreamProgress {
            input,
            bytes_read: 0,
            last_log: 0,
            every,
            level,
            start_time: Instant::now(),
        }
    }
}

impl<T: Read> Read for StreamProgress<T> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let curr_bytes_read = self.input.read(buf)?;
        self.bytes_read += curr_bytes_read as u64;
        let elapsed = Instant::now().duration_since(self.start_time).as_secs();
        let logs = elapsed / self.every as u64;
        if logs > self.last_log {
            self.last_log = logs;
            let printout = format!(
                "{} read in {} seconds @{}/sec ",
                format_size_with_unit(self.bytes_read),
                Instant::now().duration_since(self.start_time).as_secs(),
                format_size_with_unit(self.bytes_read / elapsed),
            );
            match self.level {
                Level::Trace => trace!("{}", printout),
                Level::Debug => debug!("{}", printout),
                Level::Warn => warn!("{}", printout),
                Level::Error => error!("{}", printout),
                Level::Info => info!("{}", printout),
            }
        }
        Ok(curr_bytes_read)
    }
}
