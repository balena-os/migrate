//use clap::{ArgMatches};
use failure::ResultExt;
use log::{debug, info};
use mod_logger::Logger;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use yaml_rust::{Yaml, YamlLoader};

use clap::{App, Arg};

use super::{MigErrCtx, MigError, MigErrorKind};

pub(crate) mod log_config;
pub(crate) use log_config::LogConfig;

pub(crate) mod backup_config;
pub(crate) use backup_config::BackupConfig;

pub(crate) mod migrate_config;
pub(crate) use migrate_config::{MigMode, MigrateConfig};

pub(crate) mod balena_config;
pub(crate) use balena_config::BalenaConfig;

pub mod debug_config;
pub(crate) use debug_config::DebugConfig;

use crate::{
    defs::{DEFAULT_MIGRATE_CONFIG},
    common::{
        file_exists,
        path_append,
        config_helper::{get_yaml_val},
    },
    linux_common
};

const MODULE: &str = "migrator::common::config";

// TODO: add trait ToYaml and implement for all sections

pub trait YamlConfig {
    // fn to_yaml(&self, prefix: &str) -> String;
    // fn from_yaml(&mut self, yaml: &Yaml) -> Result<(), MigError>;
    fn from_yaml(yaml: &Yaml) -> Result<Box<Self>, MigError>;
}

#[derive(Debug)]
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
                    .help("Mode of operation - agent, immediate or pretend"),
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
            .get_matches();

        let log_level = match arg_matches.occurrences_of("verbose") {
            0 => None,
            1 => Some("info"),
            2 => Some("debug"),
            _ => Some("trace"),
        };

        Logger::initialise(log_level).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            "failed to intialize logger",
        ))?;


        // try to establish work_dir and config file
        // work_dir can be specified on command line, it defaults to ./ if not
        // work_dir can also be specified in config, path specified on command line
        // will override specification in config

        // config file can be specified on command line
        // if not specified it will be looked for in ./{DEFAULT_MIGRATE_CONFIG}
        // or work_dir/{DEFAULT_MIGRATE_CONFIG}
        // If none is fouund a default is created

        let work_dir =
            if arg_matches.is_present("work_dir") {
                if let Some(dir) = arg_matches.value_of("work_dir") {
                    Some(PathBuf::from(dir)
                            .canonicalize()
                             .context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("failed to create absolute path from work_dir: '{}'", dir)))?
                    )
                } else {
                    return Err(MigError::from_remark(MigErrorKind::InvParam, "invalid command line parameter 'work_dir': no value given"));
                }
            } else {
                None
            };


        // establish a temporary working dir
        // defaults to ./ if not set above

        let tmp_work_dir =
            if let Some(ref work_dir) = work_dir {
                work_dir.clone()
            } else {
                PathBuf::from("./")
            };

        // establish a valid config path
        let config_path = {
            let config_path =
                if arg_matches.is_present("config") {
                    if let Some(cfg) = arg_matches.value_of("config") {
                        PathBuf::from(cfg)
                    } else {
                        return Err(MigError::from_remark(MigErrorKind::InvParam, "invalid command line parameter 'config': no value given"));
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

        let mut config =
            if let Some(config_path) = config_path {
                if file_exists(&config_path) {
                    Config::from_file(&config_path)?
                } else {
                    Config::default()
                }
            } else {
                Config::default()
            };

        if let Some(work_dir) = work_dir {
            // if work_dir was set in command line it overrides
            config.migrate.work_dir = work_dir;
        }

        if arg_matches.is_present("mode") {
            if let Some(mode) = arg_matches.value_of("mode") {
                config.migrate.mode = match mode.to_lowercase().as_str() {
                    "immediate" => MigMode::IMMEDIATE,
                    "agent" => MigMode::AGENT,
                    "pretend" => MigMode::PRETEND,
                    _ => {
                        return Err(MigError::from_remark(
                            MigErrorKind::InvParam,
                            &format!(
                                "{}::new: invalid value for parameter mode: '{}'",
                                MODULE, mode
                            ),
                        ));
                    }
                }
            }
        }

        debug!("{}::new: migrate mode: {:?}", MODULE, config.migrate.mode);

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

    fn get_debug_config(&mut self, yaml: &Yaml) -> Result<(), MigError> {
        if let Some(section) = get_yaml_val(yaml, &["debug"])? {
            self.debug = *DebugConfig::from_yaml(section)?;
        }
        Ok(())
    }

    /*
    fn print_debug_config(&self, prefix: &str, buffer: &mut String) -> () {
        *buffer += &self.debug.to_yaml(prefix)
    }
    */

    fn from_string(config_str: &str) -> Result<Config, MigError> {
        debug!("{}::from_string: entered", MODULE);
        let yaml_cfg = YamlLoader::load_from_str(&config_str).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("{}::from_string: failed to parse", MODULE),
        ))?;
        if yaml_cfg.len() != 1 {
            return Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::from_string: invalid number of configs in file: {}",
                    MODULE,
                    yaml_cfg.len()
                ),
            ));
        }

        Ok(*Config::from_yaml(&yaml_cfg[0])?)
    }

    fn from_file<P: AsRef<Path>>(file_name: &P) -> Result<Config, MigError> {
        let file_name = file_name.as_ref();
        debug!("{}::from_file: {} entered", MODULE, file_name.display());
        Config::from_string(&read_to_string(file_name).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("{}::from_file: failed to read {}", MODULE, file_name.display()),
        ))?)
    }

    fn check(&self) -> Result<(), MigError> {
        match self.migrate.mode {
            MigMode::AGENT => {}
            MigMode::PRETEND => {}
            MigMode::IMMEDIATE => {
                self.migrate.check(&self.migrate.mode)?;
                self.balena.check(&self.migrate.mode)?;
                self.debug.check(&self.migrate.mode)?;
            }
        }

        Ok(())
    }
}

impl YamlConfig for Config {
    fn from_yaml(yaml: &Yaml) -> Result<Box<Config>, MigError> {
        Ok(Box::new(Config{
            migrate:
                if let Some(ref section) = get_yaml_val(yaml, &["migrate"])? {
                    *MigrateConfig::from_yaml(section)?
                } else {
                    MigrateConfig::default()
                },
            balena:
                if let Some(section) = get_yaml_val(yaml, &["balena"])? {
                    // Params: balena_image
                    *BalenaConfig::from_yaml(section)?
                } else {
                    BalenaConfig::default()
                },
            debug:
                if let Some(section) = get_yaml_val(yaml, &["debug"])? {
                    *DebugConfig::from_yaml(section)?
                } else {
                    DebugConfig::default()
                },
        }))
    }
/*
    fn to_yaml(&self, prefix: &str) -> String {
        let mut output = self.migrate.to_yaml(prefix);
        if let Some(ref balena) = self.balena {
            output += &balena.to_yaml(prefix);
        }

        self.print_debug_config(prefix, &mut output);

        output
    }
*/
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: update this to current config

    const TEST_CONFIG: &str = "
migrate:
  mode: IMMEDIATE
  all_wifis: true
  reboot: 10
  log_to:
    drive: '/dev/sda1'
    fs_type: ext4
balena:
  image: image.gz
  config: config.json
";

    fn assert_test_config1(config: &Config) -> () {
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
    fn read_sample_conf() -> () {
        let mut config = Config::default();
        config.from_string(TEST_CONFIG).unwrap();
        assert_test_config1(&config);
        ()
    }

    /*
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

}
