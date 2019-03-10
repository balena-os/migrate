use win_test::mswin;
use win_test::common::{SysInfo};

extern crate lazy_static;
extern crate regex;

#[macro_use]
extern crate log;
extern crate clap;
extern crate stderrlog;

use clap::{App, Arg};

fn main() {
    trace!("balena-migrate started");
    let matches = App::new("balena-migrate")
        .version("0.1")
        .author("Thomas Runte <thomasr@balena.io>")
        .about("Migrates devices to BalenaOS")
        .arg(
            Arg::with_name("info")
                .short("i")
                .help("reports system info")
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .multiple(true)
                .help("Sets the level of verbosity"),
        )
        .get_matches();

    let log_level = matches.occurrences_of("verbose") as usize;
    
    stderrlog::new()
        .module(module_path!())
        .verbosity(log_level)
        .timestamp(stderrlog::Timestamp::Millisecond)
        .init()
        .unwrap();

    let msw_info = mswin::MSWInfo::try_init().unwrap();
    println!("OS Name:     {}",msw_info.get_os_name());
    println!("OS Release:  {}",msw_info.get_os_release());
    println!("Boot Device: {}",msw_info.get_boot_dev());
    println!("Total Mem.:  {}",msw_info.get_mem_tot());
    println!("Avail. Mem.: {}",msw_info.get_mem_avail());
}
