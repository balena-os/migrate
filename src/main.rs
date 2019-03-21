use clap::{App, Arg};

use balena_migrator::{Migrator};
use balena_migrator::mswin::win_api::{enumerate_drives};

fn print_drives() -> () {
    let drive_map = enumerate_drives().unwrap();
    for key in drive_map.keys() {
        println!("{:?}",drive_map.get(key));
    }
}

fn print_sysinfo(s_info: &mut Migrator) -> () {

    match s_info.get_os_name() {
        Ok(v)    => println!("OS Name:          {}",v),
        Err(why) => println!("OS Name:          failed: {}",why), 
    }; 

    match s_info.get_os_release() {
        Ok(v)    => println!("OS Release:       {}",v),
        Err(why) => println!("OS Release:       failed: {}",why), 
    }; 

    match s_info.get_os_arch() {
        Ok(v)    => println!("OS Architecture:  {}",v),
        Err(why) => println!("OS Architecture:  failed: {}",why), 
    }; 

    match s_info.is_uefi_boot() {
        Ok(v)    => println!("UEFI Boot:        {}",v),
        Err(why) => println!("UEFI Boot:        failed: {}",why), 
    }; 

    match s_info.get_boot_dev() {
        Ok(v)    => println!("Boot Device:      {:?}",v),
        Err(why) => println!("Boot Device:      failed: {}",why), 
    }; 

    match s_info.get_mem_tot() {
        Ok(v)    => println!("PhysicalMemory:   {:?}",v),
        Err(why) => println!("PhysicalMemory:   failed: {}",why), 
    }; 

    match s_info.get_mem_avail() {
        Ok(v)    => println!("Available Memory: {:?}",v),
        Err(why) => println!("Available Memory: failed: {}",why), 
    }; 

    match s_info.is_admin() {
        Ok(v)    => println!("Is Admin:         {}",v),
        Err(why) => println!("Is Admin:         failed: {}",why), 
    };

    match s_info.is_secure_boot() {
        Ok(v)    => println!("Is Secure Boot:   {}",v),
        Err(why) => println!("Is Secure Boot:   failed: {}",why), 
    }; 

}

fn main() {
    println!("balena-migrate-win started");
    let matches = App::new("balena-migrate-win")
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

    let mut migrator = balena_migrator::get_migrator().unwrap();
    print_sysinfo(migrator.as_mut());

    #[cfg(target_os = "windows")]
    print_drives();
}
