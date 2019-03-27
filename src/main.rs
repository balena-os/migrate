use clap::{App, Arg, ArgMatches};
use log::{info};

use balena_migrator::mig_error::MigError;

use balena_migrator::Migrator;
#[cfg(target_os = "windows")]
use balena_migrator::mswin::MSWMigrator;

const GB_SIZE: u64 = 1024 * 1024 * 1024;
const MB_SIZE: u64 = 1024 * 1024;
const KB_SIZE: u64 = 1024;

fn format_size_with_unit(size: u64) -> String {
    if size > (10 * GB_SIZE) {
        format!("{} GB", size / GB_SIZE)
    } else if size > (10 * MB_SIZE) {
        format!("{} MB", size / MB_SIZE)
    } else if size > (10 * KB_SIZE) {
        format!("{} kB", size / KB_SIZE)
    } else {
        format!("{} B", size)
    }
}


#[cfg(target_os = "windows")]
fn test_com() -> Result<(), MigError> {
    use balena_migrator::mswin::win_api::com_api::ComAPI;
    use balena_migrator::mswin::win_api::wmi_api::WmiAPI;

    info!("calling ComAPI::get_api()");
    let h_com_api = ComAPI::get_api()?;
    info!("calling WmiAPI::get_api_from_hcom");
    let wmi_api = WmiAPI::get_api_from_hcom(h_com_api)?;
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
fn print_drives(migrator: &mut MSWMigrator) -> Result<(),MigError> {
    use balena_migrator::mswin::drive_info::{enumerate_drives,DeviceProps};

    let drive_map = enumerate_drives(migrator).unwrap();
    let mut keys: Vec<&u64> = drive_map.keys().collect();
    keys.sort();

    for key in  keys {            
        let pd_info = drive_map.get(key).unwrap();
        println!("  type: PhysicalDrive");
        println!("  harddisk index:     {}", pd_info.get_index());
        println!("  device:             {}", pd_info.get_device());
        println!("  wmi name:           {}", pd_info.get_wmi_name());
        println!("  media type:         {}", pd_info.get_media_type());
        println!("  bytes per sector:   {}", pd_info.get_bytes_per_sector());
        println!("  partitions:         {}", pd_info.get_partitions());
        println!("  compression_method: {}", pd_info.get_compression_method());
        println!("  size:               {}", format_size_with_unit(pd_info.get_size()));    
        println!("  status:             {}\n", pd_info.get_status());
        
        for hd_part in pd_info.get_partition_list() {
            println!("    type: HarddiskPartition");
            println!("    harddisk index:   {}", hd_part.get_hd_index());
            println!("    partition index:  {}", hd_part.get_part_index());
            println!("    device :          {}", hd_part.get_device());
            if hd_part.has_wmi_info() {
                println!("    boot device:      {}", hd_part.is_boot_device().unwrap());
                println!("    bootable:         {}", hd_part.is_bootable().unwrap());                    
                println!("    type:             {}", hd_part.get_ptype().unwrap());
                println!("    number of blocks: {}", hd_part.get_num_blocks().unwrap());
                println!("    start offset:     {}", hd_part.get_start_offset().unwrap());
                println!("    size:             {}", format_size_with_unit(hd_part.get_size().unwrap()));
            }
            if let Some(dl) = hd_part.get_driveletter() {
                if let Ok(sizes) = hd_part.get_supported_sizes(migrator) {
                    println!("    min supp. size:   {} kB", format_size_with_unit(sizes.0));
                    println!("    max supp. size:   {} kB", format_size_with_unit(sizes.1));
                }
                println!("    drive letter:     {}:", dl);
            }
            println!();
        }

/*
        
        match info {
            StorageDevice::HarddiskPartition(hdp) => {
                let hdp = hdp.as_ref().borrow();
            }
            StorageDevice::PhysicalDrive(pd) => {
                let pd = pd.as_ref();
            }

            _ => {
                println!("  yet to be implemented\n");
            }
        }
        */
    }
    Ok(())
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

#[cfg (target_os = "windows")]
fn process(arg_matches: &ArgMatches) -> Result<(),MigError> {
    let mut migrator = MSWMigrator::try_init()?;

    if arg_matches.is_present("info") {        
        print_sysinfo(&mut migrator);
    }

    if arg_matches.is_present("drives") {
        print_drives(&mut migrator)?;
    }

    if arg_matches.is_present("wmi") {
        test_com()?;
    }

    Ok(())
} 

#[cfg (not (target_os = "windows"))]
fn process(arg_matches: &ArgMatches) -> Result<(),MigError> {
    Err(MigError::from(MigErrorKind::NotImpl))
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

    
    process(&matches).unwrap();

}
