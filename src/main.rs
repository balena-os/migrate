use clap::{App, Arg};
use log::{info};

use balena_migrator::Migrator;
use balena_migrator::mig_error::MigError;

#[cfg(target_os = "windows")]
fn test_com() -> Result<(), MigError> {
    use balena_migrator::mswin::win_api::com_api::ComAPI;
    use balena_migrator::mswin::win_api::wmi_api::WmiAPI;

    info!("calling ComAPI::get_api()");
    let h_com_api = ComAPI::get_api()?;
    info!("calling WmiAPI::get_api_from_hcom");
    let mut wmi_api = WmiAPI::get_api_from_hcom(h_com_api)?;
    let res = wmi_api.raw_query("SELECT Caption,Version,OSArchitecture, BootDevice, TotalVisibleMemorySize,FreePhysicalMemory FROM Win32_OperatingSystem")?;
    for item in res {
        info!("got item:");
        for key in item.keys() {
            info!("got item property: {} -> {:?}", key, item.get(key).unwrap());
        }
        info!("end of item\n");
    }
    Ok(())
}

#[cfg (not (target_os = "windows"))]
fn test_com() -> Result<(), MigError> {
    println!("test_com only works on windows");
    Ok(())
}

#[cfg(target_os = "windows")]
fn print_drives(migrator: &mut Migrator) -> () {
    use balena_migrator::mswin::drive_info::{DeviceProps, StorageDevice};

    let drive_map = migrator.enumerate_drives().unwrap();
    let mut keys: Vec<&String> = drive_map.keys().collect();
    keys.sort();

    for key in  keys {    
        println!("Key: {}", &key);
        let info = drive_map.get(key).unwrap();
        match info {
            StorageDevice::HarddiskPartition(hdp) => {
                let hdp = hdp.as_ref().borrow();
                println!("  type: HarddiskPartition");
                println!("  harddisk index:   {}", hdp.get_hd_index());
                println!("  partition index:  {}", hdp.get_part_index());
                println!("  device :          {}", hdp.get_device());
                if hdp.has_wmi_info() {
                    println!("  boot device:      {}", hdp.is_boot_device().unwrap());
                    println!("  bootable:         {}", hdp.is_bootable().unwrap());                    
                    println!("  type:             {}", hdp.get_ptype().unwrap());
                    println!("  number of blocks: {}", hdp.get_num_blocks().unwrap());
                    println!("  start offset:     {}", hdp.get_start_offset().unwrap());
                    println!("  size:             {} kB", hdp.get_size().unwrap()/1024);
                }
                if hdp.has_supported_sizes() {
                    println!("  min supp. size:   {} kB", hdp.get_min_supported_size().unwrap() / 1024);
                    println!("  max supp. size:   {} kB", hdp.get_max_supported_size().unwrap() / 1024);
                }
                println!();
            }
            StorageDevice::PhysicalDrive(pd) => {
                let pd = pd.as_ref();
                println!("  type: PhysicalDrive");
                println!("  harddisk index:     {}", pd.get_index());
                println!("  device:             {}", pd.get_device());
                println!("  wmi name:           {}", pd.get_wmi_name());
                println!("  media type:         {}", pd.get_media_type());
                println!("  bytes per sector:   {}", pd.get_bytes_per_sector());
                println!("  partitions:         {}", pd.get_partitions());
                println!("  compression_method: {}", pd.get_compression_method());
                println!("  size:               {} kB", pd.get_size() / 1024);
                println!("  status:             {}\n", pd.get_status());
            }

            _ => {
                println!("  yet to be implemented\n");
            }
        }
    }
}

#[cfg (not (target_os = "windows"))]
fn print_drives(_migrator: &mut Migrator) -> () {
    println!("print drives currently only works on windows");
    ()
}

fn print_sysinfo(s_info: &mut Migrator) -> () {
    match s_info.get_os_name() {
        Ok(v) => println!("OS Name:          {}", v),
        Err(why) => println!("OS Name:          failed: {}", why),
    };

    match s_info.get_os_release() {
        Ok(v) => println!("OS Release:       {}", v),
        Err(why) => println!("OS Release:       failed: {}", why),
    };

    match s_info.get_os_arch() {
        Ok(v) => println!("OS Architecture:  {}", v),
        Err(why) => println!("OS Architecture:  failed: {}", why),
    };

    match s_info.is_uefi_boot() {
        Ok(v) => println!("UEFI Boot:        {}", v),
        Err(why) => println!("UEFI Boot:        failed: {}", why),
    };

    match s_info.get_boot_dev() {
        Ok(v) => println!("Boot Device:      {:?}", v),
        Err(why) => println!("Boot Device:      failed: {}", why),
    };

    match s_info.get_mem_tot() {
        Ok(v) => println!("PhysicalMemory:   {:?}", v),
        Err(why) => println!("PhysicalMemory:   failed: {}", why),
    };

    match s_info.get_mem_avail() {
        Ok(v) => println!("Available Memory: {:?}", v),
        Err(why) => println!("Available Memory: failed: {}", why),
    };

    match s_info.is_admin() {
        Ok(v) => println!("Is Admin:         {}", v),
        Err(why) => println!("Is Admin:         failed: {}", why),
    };

    match s_info.is_secure_boot() {
        Ok(v) => println!("Is Secure Boot:   {}", v),
        Err(why) => println!("Is Secure Boot:   failed: {}", why),
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
                .help("reports system info"),
        )
        .arg(
            Arg::with_name("wmi")
                .short("w")
                .help("reports wmi infos"),
        )
        .arg(
            Arg::with_name("drives")
                .short("d")
                .help("reports system drives"),
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

    if matches.is_present("info") {        
        print_sysinfo(migrator.as_mut());
    }

    if matches.is_present("drives") {
        print_drives(migrator.as_mut());
    }

    if matches.is_present("wmi") {
        test_com().unwrap();
    }
}
