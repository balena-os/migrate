use failure::ResultExt;
use log::{debug, error, info, Level};
use mod_logger::{LogDestination, Logger, NO_STREAM};
use serde::Deserialize;
use serde_yaml;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};

use clap::{App, Arg};

use super::{MigErrCtx, MigError, MigErrorKind};

pub(crate) mod migrate_config;
pub(crate) use migrate_config::{MigMode, MigrateConfig};

pub(crate) mod balena_config;
pub(crate) use balena_config::BalenaConfig;

pub mod debug_config;
pub(crate) use debug_config::DebugConfig;

use crate::{
    common::{file_exists, path_append},
    defs::{DEFAULT_MIGRATE_CONFIG, VERSION},
};

#[derive(Debug, Deserialize)]
pub(crate) struct Config {
    pub migrate: MigrateConfig,
    pub balena: BalenaConfig,
    pub debug: DebugConfig,
}

impl<'a> Config {
    pub fn new() -> Result<Config, MigError> {
        let arg_matches = App::new("balena-migrate")
            .version(VERSION)
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
                    .help("Select balena OS image"),
            )
            .arg(
                Arg::with_name("config")
                    .short("c")
                    .long("config")
                    .value_name("FILE")
                    .help("Select balena-migrate config file"),
            )
            .arg(
                Arg::with_name("work_dir")
                    .short("w")
                    .long("work_dir")
                    .value_name("DIR")
                    .help("Select working directory"),
            )
            .arg(
                Arg::with_name("verbose")
                    .short("v")
                    .multiple(true)
                    .help("Set the level of verbosity"),
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
        // If none is found a default is created

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

            // try to locate config in absolute path or relative to tmp_work path established above
            if config_path.is_absolute() {
                Some(config_path)
            } else if let Ok(abs_path) = config_path.canonicalize() {
                Some(abs_path)
            } else if let Ok(abs_path) = path_append(tmp_work_dir, config_path).canonicalize() {
                Some(abs_path)
            } else {
                None
            }
        };

        let mut config = if let Some(config_path) = config_path {
            if file_exists(&config_path) {
                let mut config = Config::from_file(&config_path)?;
                // use config path as workdir if nothing else was defined
                if !config.migrate.has_work_dir() && work_dir.is_none() {
                    config
                        .migrate
                        .set_work_dir(config_path.parent().unwrap().to_path_buf());
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
            error!("no working directory specified and no configuration found");
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

        debug!("new: migrate mode: {:?}", config.migrate.get_mig_mode());

        if arg_matches.is_present("image") {
            if let Some(image) = arg_matches.value_of("image") {
                config.balena.set_image_path(image);
            }
        }

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

    // config is checed for validity at the end, when all has been set
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
        common::{
            config::{
                balena_config::FileRef,
                balena_config::ImageType,
                migrate_config::{DeviceSpec, MigrateWifis},
            },
            file_digest::HashInfo,
        },
        defs::FailMode,
    };
    use std::path::PathBuf;

    // TODO: update this to current config

    #[test]
    fn read_conf_ok1() {
        let config = Config::from_string(TEST_DD_CONFIG_OK).unwrap();

        assert_eq!(config.migrate.get_mig_mode(), &MigMode::Immediate);
        assert_eq!(config.migrate.get_work_dir(), Path::new("./work/"));
        match config.migrate.get_wifis() {
            MigrateWifis::List(list) => assert_eq!(list.len(), 3),
            _ => panic!("unexpected result from get_wifis"),
        };
        assert_eq!(config.migrate.get_reboot(), &Some(10));
        assert_eq!(config.migrate.get_log_console(), true);
        if let Some(dev_spec) = config.migrate.get_log_device() {
            if let DeviceSpec::DevicePath(path) = dev_spec {
                assert_eq!(path, &PathBuf::from("/dev/sda1"));
            }
        }
        assert_eq!(config.migrate.get_log_level(), "debug");
        assert_eq!(
            config.migrate.get_kernel_path(),
            &FileRef {
                path: PathBuf::from("balena.zImage"),
                hash: Some(HashInfo::Md5(String::from(
                    "f1b3e346889e190279f43e984c7b693a"
                )))
            }
        );
        assert_eq!(
            config.migrate.get_initrd_path(),
            &FileRef {
                path: PathBuf::from("balena.initrd.cpio.gz"),
                hash: Some(HashInfo::Md5(String::from(
                    "f1b3e346889e190279f43e984c7b693a"
                )))
            }
        );

        // TODO: more checks on backup
        let bckup_vols = config.migrate.get_backup_volumes();
        assert_eq!(bckup_vols.len(), 3);
        assert_eq!(bckup_vols.get(0).unwrap().volume, "test volume 1");

        assert_eq!(config.migrate.get_fail_mode(), &FailMode::Reboot);
        assert_eq!(config.migrate.get_nwmgr_files().len(), 1);
        assert_eq!(config.migrate.is_gzip_internal(), true);
        assert_eq!(
            config.migrate.get_kernel_opts(),
            Some(String::from("panic=20"))
        );
        assert_eq!(config.migrate.get_delay(), 60);
        assert_eq!(config.migrate.require_nwmgr_configs(), false);
        if let Some(watchdogs) = config.migrate.get_watchdogs() {
            assert_eq!(watchdogs.len(), 1_);
            assert_eq!(watchdogs[0].path, PathBuf::from("/dev/watchdog1"));
            assert_eq!(watchdogs[0].close, Some(false));
            assert_eq!(watchdogs[0].interval, Some(10));
        } else {
            panic!("No Watchdogs found");
        }

        if let ImageType::Flasher(comp) = config.balena.get_image_path() {
            assert_eq!(
                comp,
                &FileRef {
                    path: PathBuf::from(
                        "balena-cloud-bobdev-intel-nuc-2.39.0+rev3-dev-v10.0.3.img.gz"
                    ),
                    hash: Some(HashInfo::Md5(String::from(
                        "4834c4ffb3ee0cf0be850242a693c9b6",
                    )))
                }
            );
        } else {
            panic!("Invalid image type");
        };

        assert_eq!(
            config.balena.get_config_path(),
            &FileRef {
                path: PathBuf::from("config.json"),
                hash: Some(HashInfo::Md5(String::from(
                    "4834c4ffb3ee0cf0be850242a693c9b6"
                )))
            }
        );

        assert_eq!(config.balena.is_check_vpn(), true);
        assert_eq!(config.balena.get_check_timeout(), 20);
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
  watchdogs:
    - path: /dev/watchdog1
      interval: 10
      close: false
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
  watchdogs:
    - path: /dev/watchdog1
      interval: 10
      close: false
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
