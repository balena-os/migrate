use balena_migrator::mig_error::MigError;
use balena_migrator::mswin::win_api::com_api::get_com_api;
use balena_migrator::mswin::win_api::wmi_api::WmiAPI;
use clap::{App, Arg};
use log::info;

#[cfg(target_os = "windows")]
fn test_com() -> Result<(), MigError> {
    info!("calling ComAPI::get_api()");
    let h_com_api = get_com_api()?;
    info!("calling WmiAPI::get_api_from_hcom");
    let wmp_api = WmiAPI::get_api_from_hcom(h_com_api)?;
    Ok(())
}

fn main() {
    println!("test_com started");
    let matches = App::new("test_com")
        .version("0.1")
        .author("Thomas Runte <thomasr@balena.io>")
        .about("Test COM interfaces in mswin version of balena-migrator")
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

    info!("main: log_level: {} ", log_level);

    #[cfg(target_os = "windows")]
    test_com().unwrap();
    info!("main: done");
}
