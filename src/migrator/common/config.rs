use failure::ResultExt;
use log::{debug, error, info, Level};
use mod_logger::{LogDestination, Logger, NO_STREAM};
use serde::Deserialize;
use serde_yaml;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

use clap::{App, Arg};

use super::{MigErrCtx, MigError, MigErrorKind};

/* moved into migrate_config
pub(crate) mod volume_config;
pub(crate) use backup_config::BackupConfig;
*/
pub(crate) mod migrate_config;
pub(crate) use migrate_config::{MigMode, MigrateConfig, MigrateWifis};

pub(crate) mod balena_config;
pub(crate) use balena_config::BalenaConfig;

pub mod debug_config;
pub(crate) use debug_config::DebugConfig;

use crate::{
    common::{file_exists, path_append},
    defs::DEFAULT_MIGRATE_CONFIG,
};

const MODULE: &str = "migrator::common::config";

// TODO: add trait ToYaml and implement for all sections

/*
pub trait YamlConfig {
    // fn to_yaml(&self, prefix: &str) -> String;
    // fn from_yaml(&mut self, yaml: &Yaml) -> Result<(), MigError>;
    fn from_yaml(yaml: &Yaml) -> Result<Box<Self>, MigError>;
}
*/

#[derive(Debug, Deserialize)]
pub(crate) struct Config {
    pub migrate: MigrateConfig,
    pub balena: BalenaConfig,
    pub debug: DebugConfig,
}

impl<'a> Config {
    pub fn new() -> Result<Config, MigError> {
        let arg_matches = App::new("balena-migrate")
            .version("0.1")
            .author("Thomas Runte <thomasr@balena.io>")
            .about("Migrates devices to BalenaOS")
            .arg(
                Arg::with_name("mode")
                    .short("m")
                    .long("mode")
                    .value_name("MODE")
                    .help("Mode of operation - extract, agent, immediate or pretend"),
            )
            .arg(
                Arg::with_name("image")
                    .short("i")
                    .long("image")
                    .value_name("FILE")
                    .help("use balena OS image"),
            )
            .arg(
                Arg::with_name("config")
                    .short("c")
                    .long("config")
                    .value_name("FILE")
                    .help("use config file"),
            )
            .arg(
                Arg::with_name("work_dir")
                    .short("w")
                    .long("work_dir")
                    .value_name("DIR")
                    .help("Work directory"),
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
            .arg(
                Arg::with_name("device-type")
                    .short("d")
                    .long("device-type")
                    .value_name("type")
                    .help("specify image device slug for extraction"),
            )
            .get_matches();

        match arg_matches.occurrences_of("verbose") {
            0 => Logger::create(),
            1 => Logger::set_default_level(&Level::Info),
            2 => Logger::set_default_level(&Level::Debug),
            _ => Logger::set_default_level(&Level::Trace),
        }

        Logger::set_color(true);
        Logger::set_log_dest(&LogDestination::BufferStderr, NO_STREAM).context(
            MigErrCtx::from_remark(MigErrorKind::Upstream, "failed to set up logging"),
        )?;

        // try to establish work_dir and config file
        // work_dir can be specified on command line, it defaults to ./ if not
        // work_dir can also be specified in config, path specified on command line
        // will override specification in config

        // config file can be specified on command line
        // if not specified it will be looked for in ./{DEFAULT_MIGRATE_CONFIG}
        // or work_dir/{DEFAULT_MIGRATE_CONFIG}
        // If none is fouund a default is created

        let work_dir = if arg_matches.is_present("work_dir") {
            if let Some(dir) = arg_matches.value_of("work_dir") {
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
            let config_path = if arg_matches.is_present("config") {
                if let Some(cfg) = arg_matches.value_of("config") {
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

            if config_path.is_absolute() {
                Some(config_path)
            } else {
                if let Ok(abs_path) = config_path.canonicalize() {
                    Some(abs_path)
                } else {
                    if let Ok(abs_path) = path_append(tmp_work_dir, config_path).canonicalize() {
                        Some(abs_path)
                    } else {
                        None
                    }
                }
            }
        };

        let mut config = if let Some(config_path) = config_path {
            if file_exists(&config_path) {
                let mut config = Config::from_file(&config_path)?;
                // use config path as workdir if nothing other was defined
                if !config.migrate.has_work_dir() {
                    if let None = work_dir {
                        config
                            .migrate
                            .set_work_dir(config_path.parent().unwrap().to_path_buf());
                    }
                }
                config
            } else {
                Config::default()
            }
        } else {
            Config::default()
        };

        if let Some(work_dir) = work_dir {
            // if work_dir was set in command line it overrides
            config.migrate.set_work_dir(work_dir);
        }

        if !config.migrate.has_work_dir() {
            error!("no workdir specified and no configuration found");
            return Err(MigError::displayed());
        }

        debug!(
            "Using work_dir '{}'",
            config.migrate.get_work_dir().display()
        );

        if arg_matches.is_present("mode") {
            if let Some(mode) = arg_matches.value_of("mode") {
                config.migrate.set_mig_mode(&MigMode::from_str(mode)?);
            }
        }

        if arg_matches.is_present("image") {
            if let Some(image) = arg_matches.value_of("image") {
                config.balena.set_image_path(image);
            }
        }

        debug!(
            "{}::new: migrate mode: {:?}",
            MODULE,
            config.migrate.get_mig_mode()
        );

        debug!("{}::new: got: {:?}", MODULE, config);

        config.check()?;

        Ok(config)
    }

    fn default() -> Config {
        Config {
            migrate: MigrateConfig::default(),
            balena: BalenaConfig::default(),
            debug: DebugConfig::default(),
        }
    }

    // TODO: reimplement in Debug mode with serde (de)serialize

    /*
    fn get_debug_config(&mut self, yaml: &Yaml) -> Result<(), MigError> {
        if let Some(section) = get_yaml_val(yaml, &["debug"])? {
            self.debug = *DebugConfig::from_yaml(section)?;
        }
        Ok(())
    }


    fn print_debug_config(&self, prefix: &str, buffer: &mut String) -> () {
        *buffer += &self.debug.to_yaml(prefix)
    }
    */

    fn from_string(config_str: &str) -> Result<Config, MigError> {
        debug!("{}::from_string: entered", MODULE);
        Ok(
            serde_yaml::from_str(config_str).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                "failed to deserialze config from yaml",
            ))?,
        )
    }

    fn from_file<P: AsRef<Path>>(file_name: &P) -> Result<Config, MigError> {
        let file_name = file_name.as_ref();
        debug!("{}::from_file: {} entered", MODULE, file_name.display());
        info!("Using config file '{}'", file_name.display());
        Config::from_string(&read_to_string(file_name).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!(
                "{}::from_file: failed to read {}",
                MODULE,
                file_name.display()
            ),
        ))?)
    }

    fn check(&self) -> Result<(), MigError> {
        self.migrate.check()?;
        let mode = self.migrate.get_mig_mode();
        self.balena.check(mode)?;
        self.debug.check(mode)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        common::config::{balena_config::FileRef, migrate_config::MigrateWifis},
        defs::FailMode,
    };
    use std::path::PathBuf;

    // TODO: update this to current config

    #[test]
    fn read_conf_ok1() -> () {
        let config = Config::from_string(TEST_DD_CONFIG_OK).unwrap();

        assert_eq!(config.migrate.get_mig_mode(), &MigMode::Immediate);
        assert_eq!(config.migrate.get_work_dir(), Path::new("./work/"));
        match config.migrate.get_wifis() {
            MigrateWifis::List(list) => assert_eq!(list.len(), 3),
            _ => panic!("unexpected result from get_wifis"),
        };
        assert_eq!(config.migrate.get_reboot(), &Some(10));
        assert_eq!(
            config.migrate.get_kernel_path(),
            &FileRef {
                path: PathBuf::from("balena_x86_64.migrate.kernel"),
                hash: None
            }
        );
        assert_eq!(
            config.migrate.get_initrd_path(),
            &FileRef {
                path: PathBuf::from("balena_x86_64.migrate.initramfs"),
                hash: None
            }
        );
        assert_eq!(config.migrate.get_fail_mode(), &FailMode::Reboot);
        /*        assert_eq!(
                    config.migrate.get_force_slug(),
                    Some(String::from("dummy_device"))
                );
        */
        // TODO: more cecks on backup
        let bckup_vols = config.migrate.get_backup_volumes();
        assert_eq!(bckup_vols.len(), 3);
        assert_eq!(bckup_vols.get(0).unwrap().volume, "test volume 1");

        // assert_eq!(config.balena.get_image_path(), Path::new("image.gz"));
        assert_eq!(
            config.balena.get_config_path(),
            &FileRef {
                path: PathBuf::from("config.json"),
                hash: None
            }
        );
        /*
        assert_eq!(config.balena.get_app_name(), Some("test"));
        assert_eq!(config.balena.get_api_host(), "api1.balena-cloud.com");
        assert_eq!(config.balena.get_api_port(), 444);
        assert_eq!(config.balena.is_api_check(), false);
        assert_eq!(config.balena.get_api_key(), Some(String::from("secret")));
        */
        assert_eq!(config.balena.is_check_vpn(), false);
        assert_eq!(config.balena.get_check_timeout(), 42);

        ()
    }

    #[test]
    fn read_conf_ok2() -> () {
        let config = Config::from_string(TEST_FS_CONFIG_OK).unwrap();
    }

    /*

        fn assert_test_config_ok(config: &Config) -> () {
            match config.migrate.mode {
                MigMode::IMMEDIATE => (),
                _ => {
                    panic!("unexpected migrate mode");
                }
            };

            assert!(config.migrate.all_wifis == true);

            if let Some(i) = config.migrate.reboot {
                assert!(i == 10);
            } else {
                panic!("missing parameter migarte.reboot");
            }

            if let Some(ref log_to) = config.migrate.log_to {
                assert!(log_to.drive == "/dev/sda1");
                assert!(log_to.fs_type == "ext4");
            } else {
                panic!("no log config found");
            }

            if let Some(ref balena) = config.balena {
                assert!(balena.get_image_path().to_string_lossy() == "image.gz");
                assert!(balena.get_config_path().to_string_lossy() == "config.json");
            } else {
                panic!("no balena config found");
            }

            config.check().unwrap();
        }



        #[test]
        fn read_write() -> () {
            let mut config = Config::default();
            config.from_string(TEST_CONFIG).unwrap();

            let out = config.to_yaml("");

            let mut new_config = Config::default();
            new_config.from_string(&out).unwrap();
            assert_test_config1(&new_config);

            ()
        }
    */

    const TEST_DD_CONFIG_OK: &str = r###"
migrate:
  # mode AGENT, IMMEDIATE, PRETEND
  #  AGENT - not yet implemented, connects to balena-cloud, controlled by dashboard
  #  IMMEDIATE: migrates the device
  #   not yet implemented:
  #     if app, api, api_key, are given in balena section, config & image can be downloaded
  #  PRETEND: only validates conditions for IMMEDIATE, changes nothing
  mode: immediate
  # where all files are expected to be found
  work_dir: './work/'
  # migrate all wifi configurations found on device
  all_wifis: true
  # migrate only the following wifi ssid's (overrides all_wifis)
  wifis:
    - 'Xcover'
    - 'QIFI'
    - 'bla'
  # reboot automatically after n seconds
  reboot: 10
  # not yet implemented, subject to change
  log_to:
    drive: "/dev/sda1"
    fs_type: ext4
  # the migrate kernel, might be downloaded automatically in future versions
  kernel_path: "balena_x86_64.migrate.kernel"
  # the migrate initramfs, might be downloaded automatically in future versions
  initrd_path: "balena_x86_64.migrate.initramfs"
  # backup configuration
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
  ## what to do on a recoverable fail in phase 2, either reboot or rescueshell
  fail_mode: Reboot
  ## forced use of a device slug other than the one detected
  force_slug: 'dummy_device'
balena:
  ## the balena image version to download (not yet implemented)
  version:
  ## the balena image to flash
  image:
    dd:
      path: image.gz
  ## the balena config file to use (can be auto generated in future versions)
  config: config.json
  ## The balena app name - needed for download (not yet implemented) checked against present config.json
  app_name: 'test'
  ## Api to use for connectivity check, agent mode, downloads etc
  api:
    host: "api1.balena-cloud.com"
    port: 444
    check: false
    key: "secret"
  ## VPN to use for connectivity check
  check_vpn: false
  ## connectivity check timeout
  check_timeout: 42
  ## Api key  to use for agent mode, downloads etc
debug:
  ## flash to a device other than the boot device
  force_flash_device: '/dev/sdb'
  ## run migration up to phase2 but stop & reboot before flashing
  no_flash: true
"###;
    const TEST_FS_CONFIG_OK: &str = r###"
migrate:
  # migrate mode
  # immediate migrate
  # pretend : just run stage 1 without modifying anything
  # extract : do not migrate extract image instead
  mode: immediate
  # where required files are expected
  work_dir: '.'
  # migrate all found wifi configurations
  all_wifis: true
  # automatically reboot into stage 2 after n seconds
  reboot: 5
  log:
    # use this drive for stage2 persistent logging
    drive: '/dev/sda1'
    # stage2 log level (trace, debug, info, warn, error)
    level: 'debug'
  # path to stage2 kernel - must be a balena os kernel matching the device type
  kernel_path: 'balena.zImage'
  # path to stage2 initramfs
  initrd_path: 'balena.initramfs.cpio.gz'
  # path to stage2 device tree blob - better be a balena dtb matching the device type
  dtb_path: 'balena.dtb'
  # backup configuration, configured files are copied to balena and mounted as volumes
  backup:
  # network manager configuration files
  nwmgr_files:
    - eth0_static
    - sprint
  # use internal gzip with dd
  gzip_internal: ~
  # Extra kernel commandline options
  kernel_opts: "panic=20"
  # Use the given device instead of the boot device to flash to
  force_flash_device: ~
  # delay migration by n seconds - workaround for stem watchdog
  delay: 60
  # test kicking watchdogs - work in progress - not currently working
  watchdogs:
    # path to watchdog device
    #- path: /dev/watchdog1
      # optional interval in seconds - overrides interval read from watchdog device
      #  interval: ~
      # optional close, false disables MAGICCLOSE flag read from device
      # close: false
balena:
  image:
    # use filesystem writes instead of Flasher (dd)
    fs:
      # needed for filesystem writes, beagleboard-xm masquerades as beaglebone-black
      device_slug: beaglebone-black
      # make mkfs.ext4 check for bad blocks, either
      # empty / none, -> No test
      # ro -> Read test
      # rw -> ReadWrite test (slow)
      check: ro
      # maximise resin-data partition, true / false
      # empty / true -> maximise
      # false -> do not maximise
      # Currently required to be empty / true - migration does not succeed with max_data set to false
      max_data: true
      # use direct io for mkfs.ext (-D see manpage)
      # true -> use direct io (slow)
      # empty / false -> do not use
      mkfs_direct: ~
      # extended partition blocks
      extended_blocks: 2162688
      # boot partition blocks & tar file
      boot:
        blocks: 81920
        archive:
          path: resin-boot.tgz
          hash:
            md5: 1234567890
      # rootA partition blocks & tar file
      root_a:
        blocks: 638976
        archive:
          path: resin-rootA.tgz
      # rootB partition blocks & tar file
      root_b:
        blocks: 638976
        archive:
          path: resin-rootB.tgz
      # state partition blocks & tar file
      state:
        blocks: 40960
        archive:
          path: resin-state.tgz
      # data partition blocks & tar file
      data:
        blocks: 2105344
        archive:
          path: resin-data.tgz
  # config.json file to inject
  config: config.json
  # application name
  app_name: 'bbtest'
  # api checks
  api:
    host: "api.balena-cloud.com"
    port: 443
    check: true
  # check for vpn connection
  check_vpn: true
  # timeout for checks
  check_timeout: 20
debug:
  # don't flash device - terminate stage2 and reboot before flashing
  no_flash: false
"###;

}
