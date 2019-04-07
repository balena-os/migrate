use clap::{App, Arg, ArgMatches};
use log::{info};

mod migrator;

use migrator::{ 
    MigError, 
    Migrator,
    Config,
    YamlConfig,
    get_migrator,
};

#[cfg(target_os = "windows")]
use migrator::mswin::{
    MSWMigrator,
    WmiUtils,
};

const GB_SIZE: u64 = 1024 * 1024 * 1024;
const MB_SIZE: u64 = 1024 * 1024;
const KB_SIZE: u64 = 1024;

fn format_size_with_unit(size: u64) -> String {
    if size > (10 * GB_SIZE) {
        format!("{} GiB", size / GB_SIZE)
    } else if size > (10 * MB_SIZE) {
        format!("{} MiB", size / MB_SIZE)
    } else if size > (10 * KB_SIZE) {
        format!("{} KiB", size / KB_SIZE)
    } else {
        format!("{} B", size)
    }
}


#[cfg(target_os = "windows")]
fn test_com() -> Result<(), MigError> {
    use migrator::mswin::win_api::com_api::ComAPI;
    use migrator::mswin::win_api::wmi_api::WmiAPI;

    info!("calling ComAPI::get_api()");
    let h_com_api = ComAPI::get_api()?;
    info!("calling WmiAPI::get_api_from_hcom");
    let wmi_api = WmiAPI::get_api_from_hcom(h_com_api, "ROOT\\CVIM2")?;
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


#[cfg (not (target_os = "windows"))]
fn test() -> Result<(), MigError> {
    println!("test_com only works on windows");
    Ok(())
}

#[cfg(target_os = "windows")]
fn test(migrator: &mut MSWMigrator) -> Result<(),MigError> {
    WmiUtils::test_get_drive(0);
    Ok(())
}


#[cfg(target_os = "windows")]
fn print_drives(migrator: &mut MSWMigrator) -> Result<(),MigError> {
    use migrator::mswin::{WmiUtils};    
    
    for phys_drive in WmiUtils::query_drives()? {
        println!("  type: PhysicalDrive");
        println!("  harddisk index:     {}", phys_drive.get_index());
        println!("  device:             {}", phys_drive.get_device());
        println!("  wmi name:           {}", phys_drive.get_wmi_name());
        println!("  media type:         {}", phys_drive.get_media_type());
        println!("  bytes per sector:   {}", phys_drive.get_bytes_per_sector());
        println!("  partitions:         {}", phys_drive.get_partitions());
        println!("  compression_method: {}", phys_drive.get_compression_method());
        println!("  size:               {}", format_size_with_unit(phys_drive.get_size()));    
        println!("  status:             {}\n", phys_drive.get_status());

        for partition in phys_drive.query_partitions()? {
            println!("    type: HarddiskPartition");
            println!("    harddisk index:   {}", partition.get_hd_index());
            println!("    partition index:  {}", partition.get_part_index());
            println!("    device :          {}", partition.get_device());
            println!("    boot device:      {}", partition.is_boot_device());
            println!("    bootable:         {}", partition.is_bootable());                    
            println!("    type:             {}", partition.get_ptype());
            println!("    number of blocks: {}", partition.get_num_blocks());
            println!("    start offset:     {}", partition.get_start_offset());
            println!("    size:             {}", format_size_with_unit(partition.get_size()));
            if let Some(ld) = partition.query_logical_drive()? {
                println!("    logical drive:    {}",ld.get_device_id());
                if migrator.is_admin()? == true {
                    let supp_sizes = ld.get_supported_sizes(migrator)?;
                    println!("    min supp. size:   {}", format_size_with_unit(supp_sizes.0));
                    println!("    max supp. size:   {}", format_size_with_unit(supp_sizes.1));
                }
            }
            println!();
        }        
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

    if arg_matches.is_present("test") {
        test(&mut migrator)?;
    }

    Ok(())
} 

#[cfg (not (target_os = "windows"))]
fn process(arg_matches: &ArgMatches) -> Result<(),MigError> {
    let config = Config::new(arg_matches)?;
    println!("config out:\n{}", config.to_yaml(""));
    let _migrator = get_migrator(config)?;
    Ok(())    
} 

fn main() {
    println!("balena-migrate-win started");
    let matches = App::new("balena-migrate-win")
        .version("0.1")
        .author("Thomas Runte <thomasr@balena.io>")
        .about("Migrates devices to BalenaOS")
        .arg(Arg::with_name("immediate")
                .short("m")
                .long("immediate")
                .help("select immediate mode"),
        )
        .arg(Arg::with_name("agent")
                .short("a")
                .long("agent")
                .help("select agent mode"),
        )
        .arg(
            Arg::with_name("explain")
                .short("e")
                .long("explain")
                .help("in standalone mode - explain what migrator will do to migrate"),
        )
        .arg(
            Arg::with_name("info")
                .short("i")
                .long("info")
                .help("display system info"),
        )
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("use config file"),
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
            Arg::with_name("test")
                .short("t")
                .long("test")
                .help("tests what currently needs testing"),
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
