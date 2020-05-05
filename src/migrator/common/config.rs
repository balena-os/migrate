use failure::ResultExt;
use log::{debug, error, info, Level};
use mod_logger::{LogDestination, Logger, NO_STREAM};
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

use clap::{App, Arg};

use super::{MigErrCtx, MigError, MigErrorKind};

use crate::common::file_digest::HashInfo;
use crate::defs::{FailMode, DEFAULT_API_CHECK_TIMEOUT};
use crate::{
    common::{file_exists, path_append},
    defs::{DEFAULT_MIGRATE_CONFIG, VERSION},
};

const NO_NMGR_FILES: &[PathBuf] = &[];
const NO_BACKUP_VOLUMES: &[VolumeConfig] = &[];

const DEFAULT_CONFIG: &str = r##"
## select the working directory
# work_dir: ./

## select the mode of operation either pretend or immediate 
# mode: immediate

## set the reboot delay in seconds, default is 0 / disabled
# reboot: 0

## try to create network manager configurations for all configured wifi's
# all_wifis: true

## Specify a list of SSID's to create wifi configurations for
# wifis: 

## Setup stage2 logging to external device
# log
## set the stage2 log level, one of trace, debug, info, warn, error
#   level: info
## specify an external drive to log to - default is not set
#   drive: 
## specify drive by uuid
#      uuid: F088-D128 
## specify drive by partuuid 
#      partuuid: f4e91901-1892-44d2-b45f-6ae9f26227f4
## specify drive by device path path
#      dev_path: '/dev/sda1'
## specify drive by device label
#      label: 'LOG'

## How to fail on error in stage2 in stage2, one of reboot, shell
## choose reboot to reboot on failure, shell to display a rescue shell 
# fail_mode: reboot

## Specify a backup configuration, please see README.md for details 
# backup: 

## Supply a list of custom network manager files to be injected in balena-OS
# nwmgr_files: 

## Require a valid network manager configuration to be present
# require_nwmgr_config: true

## In stage2 delay migration by the specified amount of seconds
# delay: 0

## supply extra kernel options when booting into the migrate kernel 
# kernel_opts:
 
## Supply a file containing md5 sums of all provided files for consistency checking
# md5_sums:

## Use internal gzip instead of external gzip executable
# gzip_internal: true

## Use internal tar instead of external tar executable 
# tar_internal: true

## Supply the balena-OS image file, fs-dump or download version, default - nothing selected
## Examles:
## Select a balena OS image
## image:
##   dd:
##     file: balena-cloud-intel-nuc-2.48.0+rev3.prod.img.gz
## Select a specific balena OS version for download
## image:
##   dd:
##     version: 2.48.0+rev3.prod

## Supply the config.json file
# config: 

## check balena api connectivity to api endpoint specified in config.json
# check_api: true

## check balena vpn connectivity to vpn endpoint specified in config.json
# check_vpn: true

## specify check timeout in seconds for api and vpn checks
# check_timeout: 20

## configure a device that balena-OS will be flashed to
## example:
## force_flash_device: /dev/sda
# force_flash_device: 

## Do not flash balena-OS in stage 2
# no_flash: false 

## do not fail on untested OS 
# no_os_check: false
"##;

#[derive(Debug, PartialEq, Deserialize, Clone)]
pub(crate) enum MigMode {
    //    #[serde(rename = "agent")]
    //    Agent,
    #[serde(rename = "immediate")]
    Immediate,
    #[serde(rename = "pretend")]
    Pretend,
}

const DEFAULT_MIG_MODE: MigMode = MigMode::Pretend;

#[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
pub(crate) enum UEnvStrategy {
    #[serde(rename = "uname")]
    UName,
    #[serde(rename = "manual")]
    Manual,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct UBootCfg {
    pub strategy: Option<UEnvStrategy>,
    pub mmc_index: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ItemConfig {
    pub source: String,
    pub target: Option<String>,
    // TODO: filter.allow, filter.deny
    pub filter: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct VolumeConfig {
    pub volume: String,
    pub items: Vec<ItemConfig>,
}

#[derive(Debug, PartialEq, Clone)]
pub(crate) enum MigrateWifis {
    None,
    All,
    List(Vec<String>),
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) enum DeviceSpec {
    #[serde(rename = "uuid")]
    Uuid(String),
    #[serde(rename = "partuuid")]
    PartUuid(String),
    #[serde(rename = "devpath")]
    DevicePath(PathBuf),
    #[serde(rename = "path")]
    Path(PathBuf),
    #[serde(rename = "label")]
    Label(String),
}

#[derive(Debug, Deserialize)]
pub(crate) struct LogConfig {
    pub level: Option<String>,
    pub drive: Option<DeviceSpec>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct PartDump {
    pub blocks: u64,
    pub archive: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) enum PartCheck {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "ro")]
    ReadOnly,
    #[serde(rename = "rw")]
    ReadWrite,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct FSDump {
    pub extended_blocks: u64,
    pub device_slug: String,
    pub check: Option<PartCheck>,
    pub max_data: Option<bool>,
    pub mkfs_direct: Option<bool>,
    pub boot: PartDump,
    pub root_a: PartDump,
    pub root_b: PartDump,
    pub state: PartDump,
    pub data: PartDump,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub(crate) struct FileRef {
    pub path: PathBuf,
    pub hash: Option<HashInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) enum ImageSource {
    #[serde(rename = "file")]
    File(PathBuf),
    #[serde(rename = "version")]
    Version(String),
}

#[allow(clippy::large_enum_variant)] //TODO refactor to remove clippy warning
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) enum ImageType {
    #[serde(rename = "dd")]
    Flasher(ImageSource),
    #[serde(rename = "fs")]
    FileSystems(FSDump),
}

#[derive(Debug, Deserialize)]
pub(crate) struct Config {
    work_dir: Option<PathBuf>,
    mode: Option<MigMode>,
    reboot: Option<u64>,
    all_wifis: Option<bool>,
    wifis: Option<Vec<String>>,
    log: Option<LogConfig>,
    // TODO: check fail mode processing
    fail_mode: Option<FailMode>,
    backup: Option<Vec<VolumeConfig>>,
    // TODO: find a good way to do digests on NetworkManager files
    nwmgr_files: Option<Vec<PathBuf>>,
    require_nwmgr_config: Option<bool>,
    delay: Option<u64>,
    kernel_opts: Option<String>,
    uboot: Option<UBootCfg>,
    md5_sums: Option<PathBuf>,
    tar_internal: Option<bool>,

    image: Option<ImageType>,
    config: Option<PathBuf>,
    // app_name: Option<String>,
    check_api: Option<bool>,
    check_vpn: Option<bool>,
    check_timeout: Option<u64>,

    force_flash_device: Option<PathBuf>,
    // pretend mode, stop after unmounting former root
    no_flash: Option<bool>,
    // free form debug parameters, eg. dump-efi
    hacks: Option<Vec<String>>,

    gzip_internal: Option<bool>,
    no_os_check: Option<bool>,
    migrate_hostname: Option<bool>,
}

impl<'a> Config {
    pub fn new() -> Result<Config, MigError> {
        let arg_matches = App::new("balena-migrate")
            .version(VERSION)
            .author("Thomas Runte <thomasr@balena.io>")
            .about("Migrate a device to BalenaOS")
            .arg(
                Arg::with_name("pretend")
                    .short("p")
                    .long("pretend")
                    .help("Run in pretend mode - only check requirements, don't migrate"),
            )
            .arg(
                Arg::with_name("reboot")
                    .short("r")
                    .long("reboot")
                    .value_name("DELAY")
                    .help("Reboot automatically after DELAY seconds after migrate setup has succeeded"),
            )
            .arg(
                Arg::with_name("version")
                    .long("version")
                    .value_name("VERSION")
                    .help("Select balena OS image version for download"),
            )
            .arg(
                Arg::with_name("image")
                    .short("i")
                    .long("image")
                    .value_name("FILE")
                    .help("Select balena OS image"),
            )
            .arg(
                Arg::with_name("config-json")
                    .short("c")
                    .long("config-json")
                    .value_name("FILE")
                    .help("Select balena config.json"),
            )
            .arg(
                Arg::with_name("migrate-config")
                    .long("migrate-config")
                    .value_name("FILE")
                    .help("Select migrator config file"),
            )
            .arg(
                Arg::with_name("work-dir")
                    .short("w")
                    .long("work-dir")
                    .value_name("DIR")
                    .help("Select working directory"),
            )
            .arg(
                Arg::with_name("no-nwmgr-cfg")
                    .short("n")
                    .long("no-nwmgr-cfg")
                    .help("Allow migration without network config"),
            )
            .arg(
                Arg::with_name("no-flash")
                    .long("no-flash")
                    .help("Debug mode - do not flash in stage 2"),
            )
            .arg(
                Arg::with_name("verbose")
                    .short("v")
                    .multiple(true)
                    .help("Increase the level of verbosity"),
            )
            .arg(
                Arg::with_name("no-os-check")
                    .long("no-os-check")
                    .help("Do not fail on un-tested OS version"),
            )
            .arg(
                Arg::with_name("default-config")
                    .short("d")
                    .long("def-config")
                    .help("Print a default migrate config to stdout"),
            )
            .arg(
                Arg::with_name("balena-hostname")
                    .short("b")
                    .long("balena-hostname")
                    .help("Generate a balena hostname from device uuid rather than keeping the existing hostname"),
            )
            .get_matches();

        match arg_matches.occurrences_of("verbose") {
            0 => Logger::set_default_level(&Level::Info),
            1 => Logger::set_default_level(&Level::Debug),
            _ => Logger::set_default_level(&Level::Trace),
        }

        Logger::set_color(true);
        Logger::set_log_dest(&LogDestination::BufferStderr, NO_STREAM).context(
            MigErrCtx::from_remark(MigErrorKind::Upstream, "failed to set up logging"),
        )?;

        if arg_matches.is_present("default-config") {
            println!("{}", DEFAULT_CONFIG);
            return Err(MigError::displayed());
        }

        // try to establish work_dir and config file
        // work_dir can be specified on command line, it defaults to ./ if not
        // work_dir can also be specified in config, path specified on command line
        // will override specification in config

        // config file can be specified on command line
        // if not specified it will be looked for in ./{DEFAULT_MIGRATE_CONFIG}
        // or work_dir/{DEFAULT_MIGRATE_CONFIG}
        // If none is found a default is created

        let work_dir = if arg_matches.is_present("work-dir") {
            if let Some(dir) = arg_matches.value_of("work-dir") {
                Some(
                    PathBuf::from(dir)
                        .canonicalize()
                        .context(MigErrCtx::from_remark(
                            MigErrorKind::Upstream,
                            &format!("failed to create absolute path from work_dir: '{}'", dir),
                        ))?,
                )
            } else {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    "invalid command line parameter 'work_dir': no value given",
                ));
            }
        } else {
            None
        };

        // establish a temporary working dir
        // defaults to ./ if not set above

        let tmp_work_dir = if let Some(ref work_dir) = work_dir {
            work_dir.clone()
        } else {
            PathBuf::from("./")
        };

        // establish a valid config path
        let config_path = {
            let config_path = if arg_matches.is_present("migrate-config") {
                if let Some(cfg) = arg_matches.value_of("migrate-config") {
                    PathBuf::from(cfg)
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        "invalid command line parameter 'config': no value given",
                    ));
                }
            } else {
                PathBuf::from(DEFAULT_MIGRATE_CONFIG)
            };

            // try to locate config in absolute path or relative to tmp_work path established above
            if config_path.is_absolute() {
                Some(config_path)
            } else if let Ok(abs_path) = config_path.canonicalize() {
                Some(abs_path)
            } else if let Ok(abs_path) = path_append(&tmp_work_dir, config_path).canonicalize() {
                Some(abs_path)
            } else {
                None
            }
        };

        let mut config = if let Some(config_path) = config_path {
            if file_exists(&config_path) {
                Config::from_file(&config_path)?
            // use config path as workdir if nothing else was defined
            //if !config.has_work_dir() && work_dir.is_none() {
            //    config.set_work_dir(config_path.parent().unwrap().to_path_buf());
            //}
            // config
            } else {
                Config::default()
            }
        } else {
            Config::default()
        };

        if let Some(work_dir) = work_dir {
            // if work_dir was set in command line it overrides
            config.set_work_dir(work_dir);
        } else {
            config.set_work_dir(tmp_work_dir.canonicalize().context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("Failed to canonicalize path '{}'", tmp_work_dir.display()),
            ))?);
        }

        if !config.has_work_dir() {
            error!("no working directory specified and no configuration found");
            return Err(MigError::displayed());
        }

        debug!("Using work_dir '{}'", config.get_work_dir().display());

        if arg_matches.is_present("reboot") {
            if let Some(delay) = arg_matches.value_of("reboot") {
                config.set_reboot(delay.parse::<u64>().context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("Failed to parse reboot delay from '{}'", delay),
                ))?);
            }
        }

        if arg_matches.is_present("balena-hostname") {
            config.set_migrate_hostname(false);
        }

        if arg_matches.is_present("no-nwmgr-cfg") {
            config.set_require_nwmgr_configs(false);
        }

        if arg_matches.is_present("no-flash") {
            config.set_no_flash(true);
        }

        if arg_matches.is_present("config-json") {
            if let Some(path_str) = arg_matches.value_of("config-json") {
                config.set_config_path(&PathBuf::from(path_str));
            }
        }

        if arg_matches.is_present("pretend") {
            config.set_mig_mode(&MigMode::Pretend);
        } else {
            config.set_mig_mode(&MigMode::Immediate);
        }

        debug!("new: migrate mode: {:?}", config.get_mig_mode());

        if arg_matches.is_present("image") {
            if let Some(image) = arg_matches.value_of("image") {
                config.set_image_path(ImageSource::File(PathBuf::from(image)));
            }
        }

        if arg_matches.is_present("version") {
            if let Some(version) = arg_matches.value_of("version") {
                config.set_image_path(ImageSource::Version(version.to_string()));
            }
        }

        config.check()?;

        Ok(config)
    }

    fn default() -> Config {
        Config {
            work_dir: None,
            mode: Some(DEFAULT_MIG_MODE.clone()),
            reboot: None,
            all_wifis: None,
            wifis: None,
            log: None,
            // device_tree: None,
            fail_mode: None,
            backup: None,
            nwmgr_files: None,
            require_nwmgr_config: None,
            delay: None,
            kernel_opts: None,
            uboot: None,
            md5_sums: None,
            tar_internal: None,

            image: None,
            config: None,
            // app_name: None,
            check_api: None,
            check_vpn: None,
            check_timeout: None,

            force_flash_device: None,
            no_flash: None,
            hacks: None,
            gzip_internal: None,
            no_os_check: None,
            migrate_hostname: None,
        }
    }

    fn from_string(config_str: &str) -> Result<Config, MigError> {
        Ok(
            serde_yaml::from_str(config_str).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                "failed to deserialze config from yaml",
            ))?,
        )
    }

    fn from_file<P: AsRef<Path>>(file_name: &P) -> Result<Config, MigError> {
        let file_name = file_name.as_ref();
        info!("Using config file '{}'", file_name.display());
        Config::from_string(&read_to_string(file_name).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("from_file: failed to read {}", file_name.display()),
        ))?)
    }

    // config is checked for validity at the end, when all has been set
    fn check(&self) -> Result<(), MigError> {
        if let Some(ref uboot_cfg) = self.uboot {
            if let Some(mmc_index) = uboot_cfg.mmc_index {
                if mmc_index != 0 && mmc_index != 1 {
                    error!("mmc_index must be 0, 1, or undefined, found {}", mmc_index);
                    return Err(MigError::displayed());
                }
            }
        }

        let mode = self.get_mig_mode();
        match mode {
            //MigMode::Agent => Err(MigError::from(MigErrorKind::NotImpl)),
            _ => {
                if self.work_dir.is_none() {
                    error!("A required parameter was not found: 'work_dir'");
                    return Err(MigError::displayed());
                }
            }
        }

        if let MigMode::Immediate = mode {
            if self.config.is_none() {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    "check: no config.json was specified in mode: IMMEDIATE",
                ));
            }
        }

        Ok(())
    }

    /*****************************************
     * config migrate accessors
     *****************************************/
    pub fn set_migrate_hostname(&mut self, flag: bool) {
        self.migrate_hostname = Some(flag);
    }

    pub fn is_migrate_hostname(&self) -> bool {
        if let Some(val) = self.migrate_hostname {
            val
        } else {
            true
        }
    }

    pub fn is_tar_internal(&self) -> bool {
        if let Some(val) = self.tar_internal {
            val
        } else {
            false
        }
    }

    pub fn get_backup_volumes(&'a self) -> &'a [VolumeConfig] {
        if let Some(ref val) = self.backup {
            val.as_ref()
        } else {
            NO_BACKUP_VOLUMES
        }
    }

    pub fn set_require_nwmgr_configs(&mut self, flag: bool) {
        self.require_nwmgr_config = Some(flag);
    }

    pub fn require_nwmgr_configs(&self) -> bool {
        if let Some(val) = self.require_nwmgr_config {
            return val;
        }
        true
    }

    pub fn get_nwmgr_files(&'a self) -> &'a [PathBuf] {
        if let Some(ref val) = self.nwmgr_files {
            return val.as_slice();
        }
        NO_NMGR_FILES
    }

    pub fn set_mig_mode(&mut self, mode: &MigMode) {
        self.mode = Some(mode.clone());
    }

    pub fn get_mig_mode(&'a self) -> &'a MigMode {
        if let Some(ref mode) = self.mode {
            mode
        } else {
            &DEFAULT_MIG_MODE
        }
    }

    pub fn get_delay(&self) -> u64 {
        if let Some(val) = self.delay {
            val
        } else {
            0
        }
    }

    #[allow(dead_code)]
    pub fn get_uboot_cfg(&'a self) -> Option<&'a UBootCfg> {
        if let Some(ref val) = self.uboot {
            Some(val)
        } else {
            None
        }
    }

    pub fn get_kernel_opts(&self) -> Option<String> {
        if let Some(ref val) = self.kernel_opts {
            Some(val.clone())
        } else {
            None
        }
    }

    pub fn set_reboot(&mut self, delay: u64) {
        self.reboot = Some(delay)
    }

    pub fn get_reboot(&'a self) -> &'a Option<u64> {
        &self.reboot
    }

    pub fn get_fail_mode(&'a self) -> &'a FailMode {
        if let Some(ref val) = self.fail_mode {
            val
        } else {
            FailMode::get_default()
        }
    }

    pub fn get_wifis(&self) -> MigrateWifis {
        if let Some(ref wifis) = self.wifis {
            MigrateWifis::List(wifis.clone())
        } else if let Some(ref all_wifis) = self.all_wifis {
            if *all_wifis {
                MigrateWifis::All
            } else {
                MigrateWifis::None
            }
        } else {
            MigrateWifis::All
        }
    }

    pub fn set_work_dir(&mut self, work_dir: PathBuf) {
        self.work_dir = Some(work_dir);
    }

    pub fn has_work_dir(&self) -> bool {
        if let Some(ref _dummy) = self.work_dir {
            true
        } else {
            false
        }
    }

    pub fn get_log_device(&'a self) -> Option<&'a DeviceSpec> {
        if let Some(ref log_info) = self.log {
            if let Some(ref val) = log_info.drive {
                return Some(val);
            }
        }
        None
    }

    pub fn get_log_level(&'a self) -> &'a str {
        if let Some(ref log_info) = self.log {
            if let Some(ref val) = log_info.level {
                return val;
            }
        }
        "warn"
    }

    // The following functions can only be safely called after check has succeeded

    pub fn get_work_dir(&'a self) -> &'a Path {
        if let Some(ref dir) = self.work_dir {
            dir
        } else {
            panic!("work_dir is not set");
        }
    }

    pub fn get_md5_sums(&'a self) -> Option<PathBuf> {
        if let Some(ref dir) = self.md5_sums {
            Some(dir.clone())
        } else {
            None
        }
    }

    /*****************************************
     * config balena accessors
     *****************************************/

    pub fn is_check_vpn(&self) -> bool {
        if let Some(ref check_vpn) = self.check_vpn {
            *check_vpn
        } else {
            true
        }
    }

    pub fn is_check_api(&self) -> bool {
        if let Some(ref check_api) = self.check_api {
            *check_api
        } else {
            true
        }
    }

    pub fn get_check_timeout(&self) -> u64 {
        if let Some(timeout) = self.check_timeout {
            timeout
        } else {
            DEFAULT_API_CHECK_TIMEOUT
        }
    }

    pub fn set_image_path(&mut self, image_path: ImageSource) {
        self.image = Some(ImageType::Flasher(image_path));
    }

    // The following functions can only be safely called after check has succeeded

    pub fn get_image_path(&'a self) -> ImageType {
        if let Some(ref image) = self.image {
            image.clone()
        } else {
            ImageType::Flasher(ImageSource::Version(String::from("default")))
        }
    }

    pub fn set_config_path(&mut self, config_path: &Path) {
        self.config = Some(config_path.to_path_buf());
    }

    pub fn get_config_path(&'a self) -> &'a PathBuf {
        if let Some(ref path) = self.config {
            path
        } else {
            panic!("The balena config.json path is not set in config");
        }
    }

    /*****************************************
     * config debug accessors
     *****************************************/

    pub fn set_no_flash(&mut self, no_flash: bool) {
        self.no_flash = Some(no_flash);
    }

    pub fn is_no_flash(&self) -> bool {
        if let Some(val) = self.no_flash {
            val
        } else {
            // TODO: change to false when mature
            false
        }
    }

    pub fn is_gzip_internal(&self) -> bool {
        if let Some(val) = self.gzip_internal {
            val
        } else {
            true
        }
    }

    pub fn is_no_os_check(&self) -> bool {
        if let Some(val) = self.no_os_check {
            val
        } else {
            false
        }
    }

    pub fn get_hacks(&'a self) -> Option<&'a Vec<String>> {
        if let Some(ref val) = self.hacks {
            Some(val)
        } else {
            None
        }
    }

    #[allow(dead_code)]
    pub fn get_hack(&'a self, param: &str) -> Option<&'a String> {
        if let Some(ref hacks) = self.hacks {
            if let Some(hack) = hacks
                .iter()
                .find(|hack| (hack.as_str() == param) || hack.starts_with(&format!("{}:", param)))
            {
                Some(hack)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn get_force_flash_device(&'a self) -> Option<&'a Path> {
        if let Some(ref val) = self.force_flash_device {
            Some(val)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        common::config::{DeviceSpec, ImageType, MigrateWifis},
        defs::FailMode,
    };
    use std::path::PathBuf;

    // TODO: update this to current config

    #[test]
    fn read_conf_ok1() {
        let config = Config::from_string(TEST_DD_CONFIG_OK).unwrap();

        assert_eq!(config.get_mig_mode(), &MigMode::Immediate);
        assert_eq!(config.get_work_dir(), Path::new("./work/"));
        match config.get_wifis() {
            MigrateWifis::List(list) => assert_eq!(list.len(), 3),
            _ => panic!("unexpected result from get_wifis"),
        };
        assert_eq!(config.get_reboot(), &Some(10));
        if let Some(dev_spec) = config.get_log_device() {
            if let DeviceSpec::DevicePath(path) = dev_spec {
                assert_eq!(path, &PathBuf::from("/dev/sda1"));
            }
        }
        assert_eq!(config.get_log_level(), "debug");

        // TODO: more checks on backup
        let bckup_vols = config.get_backup_volumes();
        assert_eq!(bckup_vols.len(), 3);
        assert_eq!(bckup_vols.get(0).unwrap().volume, "test volume 1");

        assert_eq!(config.get_fail_mode(), &FailMode::Reboot);
        assert_eq!(config.get_nwmgr_files().len(), 1);
        assert_eq!(config.is_gzip_internal(), true);
        assert_eq!(config.get_kernel_opts(), Some(String::from("panic=20")));
        assert_eq!(config.get_delay(), 60);
        assert_eq!(config.require_nwmgr_configs(), false);

        if let ImageType::Flasher(comp) = config.get_image_path() {
            if let ImageSource::File(file) = comp {
                assert_eq!(
                    file,
                    PathBuf::from("balena-cloud-bobdev-intel-nuc-2.39.0+rev3-dev-v10.0.3.img.gz")
                );
            } else {
                panic!("Invalid image type");
            }
        } else {
            panic!("Invalid image type");
        }

        assert_eq!(config.get_config_path(), &PathBuf::from("config.json"));

        assert_eq!(config.is_check_vpn(), true);
        assert_eq!(config.get_check_timeout(), 20);
    }

    #[test]
    fn read_conf_ok2() -> () {
        let _config = Config::from_string(TEST_FS_CONFIG_OK).unwrap();
    }

    /*
            fn assert_test_config_ok(config: &Config) -> () {
                match config.migrate.mode {
                    MigMode::IMMEDIATE => (),
                    _ => {
                        panic!("unexpected migrate mode");
        fn read_conf_ok2() {
            // same as above except for fs tpe image so just test image
            let config = Config::from_string(TEST_FS_CONFIG_OK).unwrap();
            if let ImageType::FileSystems(dump) = config.balena.get_image_path() {
                assert_eq!(dump.device_slug, String::from("beaglebone-black"));
                assert_eq!(dump.extended_blocks, 2162688);
                assert_eq!(dump.max_data, Some(true));
                assert_eq!(dump.mkfs_direct, Some(true));
                if let Some(part_check) = &dump.check {
                    if let PartCheck::ReadOnly = part_check {
                    } else {
                        panic!("wrong PartCheck")
                    }
                } else {
                    panic!("PartCheck missing")
                }

                assert_eq!(dump.boot.blocks, 81920);
                assert_eq!(
                    dump.boot.archive,
                    FileRef {
                        path: PathBuf::from("resin-boot.tgz"),
                        hash: Some(HashInfo::Md5(String::from("1234567890")))
                    }
                );
                assert_eq!(dump.root_a.blocks, 638976);
                assert_eq!(
                    dump.root_a.archive,
                    FileRef {
                        path: PathBuf::from("resin-rootA.tgz"),
                        hash: None
                    }
                );
                assert_eq!(dump.root_b.blocks, 638976);
                assert_eq!(
                    dump.root_b.archive,
                    FileRef {
                        path: PathBuf::from("resin-rootB.tgz"),
                        hash: None
                    }
                );
                assert_eq!(dump.state.blocks, 40960);
                assert_eq!(
                    dump.state.archive,
                    FileRef {
                        path: PathBuf::from("resin-state.tgz"),
                        hash: None
                    }
                );
                assert_eq!(dump.data.blocks, 2105344);
                assert_eq!(
                    dump.data.archive,
                    FileRef {
                        path: PathBuf::from("resin-data.tgz"),
                        hash: None
                    }
                );
            } else {
                panic!("Invalid image type");
            };
        }
    */
    const TEST_DD_CONFIG_OK: &str = r###"
migrate:
  ## migrate mode
  mode: immediate
  work_dir: ./work
  all_wifis: false
  wifis:
    - 'Xcover'
    - 'QIFI'
    - 'bla'
  reboot: 10
  log:
    drive: "/dev/sda1"
    level: debug
  kernel: 
    path: balena.zImage
    hash: 
      md5: f1b3e346889e190279f43e984c7b693a
  initrd: 
    path: balena.initrd.cpio.gz
    hash:
      md5: f1b3e346889e190279f43e984c7b693a
  backup:
    - volume: "test volume 1"
      items:
      - source: /home/thomas/develop/balena.io/support
        target: "target dir 1.1"
      - source: "/home/thomas/develop/balena.io/customer/sonder/unitdata/UnitData files"
        target: "target dir 1.2"
    - volume: "test volume 2"
      items:
      - source: "/home/thomas/develop/balena.io/migrate/migratecfg/balena-migrate"
        target: "target file 2.1"
      - source: "/home/thomas/develop/balena.io/migrate/migratecfg/init-scripts"
        target: "target dir 2.2"
        filter: 'balena-.*'
    - volume: "test_volume_3"
      items:
      - source: "/home/thomas/develop/balena.io/migrate/migratecfg/init-scripts"
        filter: 'balena-.*'
  fail_mode: Reboot
  nwmgr_files:
    - eth0_static
  gzip_internal: true
  kernel_opts: "panic=20"
  delay: 60
  require_nwmgr_config: false
balena:
  image:
    dd:
      path: balena-cloud-bobdev-intel-nuc-2.39.0+rev3-dev-v10.0.3.img.gz
      hash:
        md5: 4834c4ffb3ee0cf0be850242a693c9b6
  config: 
    path: config.json
    hash:
      md5: 4834c4ffb3ee0cf0be850242a693c9b6    
  app_name: support-multi
  api:
    host: api.balena-cloud.com
    port: 443
    check: true
  check_vpn: true
  check_timeout: 20
debug:
  no_flash: true
  force_flash_device: '/dev/sdb'
"###;
    const TEST_FS_CONFIG_OK: &str = r###"
migrate:
  ## migrate mode
  mode: immediate
  work_dir: ./work
  all_wifis: false
  wifis:
    - 'Xcover'
    - 'QIFI'
    - 'bla'
  reboot: 10
  log:
    console: true
    drive: "/dev/sda1"
    level: debug
  kernel: 
    path: balena.zImage
    hash: 
      md5: f1b3e346889e190279f43e984c7b693a
  initrd: 
    path: balena.initrd.cpio.gz
    hash:
      md5: f1b3e346889e190279f43e984c7b693a
  backup:
    - volume: "test volume 1"
      items:
      - source: /home/thomas/develop/balena.io/support
        target: "target dir 1.1"
      - source: "/home/thomas/develop/balena.io/customer/sonder/unitdata/UnitData files"
        target: "target dir 1.2"
    - volume: "test volume 2"
      items:
      - source: "/home/thomas/develop/balena.io/migrate/migratecfg/balena-migrate"
        target: "target file 2.1"
      - source: "/home/thomas/develop/balena.io/migrate/migratecfg/init-scripts"
        target: "target dir 2.2"
        filter: 'balena-.*'
    - volume: "test_volume_3"
      items:
      - source: "/home/thomas/develop/balena.io/migrate/migratecfg/init-scripts"
        filter: 'balena-.*'
  fail_mode: Reboot
  nwmgr_files:
    - eth0_static
  gzip_internal: true
  kernel_opts: "panic=20"
  delay: 60
  require_nwmgr_config: false
balena:
  image:
    fs:
      device_slug: beaglebone-black
      check: ro
      max_data: true
      mkfs_direct: true
      extended_blocks: 2162688
      boot:
        blocks: 81920
        archive:
          path: resin-boot.tgz
          hash:
            md5: 1234567890
      root_a:
        blocks: 638976
        archive:
          path: resin-rootA.tgz
      root_b:
        blocks: 638976
        archive:
          path: resin-rootB.tgz
      state:
        blocks: 40960
        archive:
          path: resin-state.tgz
      data:
        blocks: 2105344
        archive:
          path: resin-data.tgz
  config: 
    path: config.json
    hash:
      md5: 4834c4ffb3ee0cf0be850242a693c9b6    
  app_name: support-multi
  api:
    host: api.balena-cloud.com
    port: 443
    check: true
  check_vpn: true
  check_timeout: 20
debug:
  no_flash: true
  force_flash_device: '/dev/sdb'
"###;
}
