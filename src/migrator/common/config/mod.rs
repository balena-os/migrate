//use clap::{ArgMatches};
use failure::ResultExt;
use log::{debug, info};
use std::fs::read_to_string;
use std::path::{Path};
use yaml_rust::{Yaml, YamlLoader};

use clap::{App, Arg};

use crate::migrator::{MigErrCtx, MigError, MigErrorKind};

use super::logger::Logger;

pub mod log_config;
pub use log_config::LogConfig;

pub mod migrate_config;
pub use migrate_config::{MigMode, MigrateConfig};

pub mod balena_config;
pub use balena_config::BalenaConfig;

#[cfg(debug_assertions)]
pub mod debug_config;
#[cfg(debug_assertions)]
pub use debug_config::DebugConfig;

const MODULE: &str = "migrator::common::config";

// TODO: add trait ToYaml and implement for all sections

pub trait YamlConfig {
    fn to_yaml(&self, prefix: &str) -> String;
    fn from_yaml(&mut self, yaml: &Yaml) -> Result<(), MigError>;
}

#[derive(Debug)]
pub struct Config {
    pub migrate: MigrateConfig,
    pub balena: Option<BalenaConfig>,
    #[cfg(debug_assertions)]
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

        Logger::initialise(arg_matches.occurrences_of("verbose") as usize)?;

        /*

                stderrlog::new()
                    .module(module_path!())
                    .verbosity(log_level)
                    .timestamp(stderrlog::Timestamp::Millisecond)
                    .init()
                    .unwrap();

                println!("log mode initialized to {}", log_level);
                info!("initialized logging");
        */

        // defaults to
        let mut config = Config::default();

        if arg_matches.is_present("config") {
            if let Some(cfg) = arg_matches.value_of("config") {
                info!("reading config from default location: '{}'", cfg);
                config.from_file(cfg)?;
            }
        } else {            
            let work_dir = 
                if arg_matches.is_present("work_dir") {
                    if let Some(dir) = arg_matches.value_of("work_dir") {
                        dir
                    } else {
                        "./"
                    }
                } else {
                    "./"
                };
                                    
            let config_path = 
                if work_dir.ends_with("/") {
                    format!("{}balena-migrate.yml", work_dir)
                } else {
                    format!("{}/balena-migrate.yml", work_dir)
                };

            debug!("{}::new: no config option given, looking for default in '{}'", MODULE, config_path);  
            if Path::new(&config_path).exists() {
                info!("reading config from default location: '{}'", config_path);
                config.from_file(&config_path)?;
            }    
        }


        if arg_matches.is_present("work_dir") {
            if let Some(work_dir) = arg_matches.value_of("work_dir") {
                config.migrate.work_dir = String::from(work_dir);
            }
        }

        if arg_matches.is_present("mode") {
            if let Some(mode) = arg_matches.value_of("mode") {
                config.migrate.mode = match mode {
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
            balena: None,
            #[cfg(debug_assertions)]
            debug: DebugConfig::default(),
        }
    }

    #[cfg(debug_assertions)]
    fn get_debug_config(&mut self, yaml: &Yaml) -> Result<(), MigError> {
        if let Some(section) = get_yaml_val(yaml, &["debug"])? {
            self.debug.from_yaml(section)?
        }
        Ok(())
    }

    #[cfg(debug_assertions)]
    fn print_debug_config(&self, prefix: &str, buffer: &mut String) -> () {
        *buffer += &self.debug.to_yaml(prefix)
    }

    fn from_string(&mut self, config_str: &str) -> Result<(), MigError> {
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

        self.from_yaml(&yaml_cfg[0])
    }

    fn from_file(&mut self, file_name: &str) -> Result<(), MigError> {
        debug!("{}::from_file: {} entered", MODULE, file_name);

        self.from_string(&read_to_string(file_name).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("{}::from_file: failed to read {}", MODULE, file_name),
        ))?)
    }

    fn check(&self) -> Result<(), MigError> {
        match self.migrate.mode {
            MigMode::AGENT => {}
            MigMode::PRETEND => {}
            MigMode::IMMEDIATE => {
                if let Some(balena) = &self.balena {
                    balena.check(&self.migrate.mode)?;
                } else {
                    return Err(MigError::from_remark(
                        MigErrorKind::InvParam,
                        &format!(
                            "{}::check: no balena section was specified in mode: IMMEDIATE",
                            MODULE
                        ),
                    ));
                }
            }
            MigMode::INVALID => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!("{}::check: no migrate mode was selected", MODULE),
                ));
            }
        }

        Ok(())
    }
}

impl YamlConfig for Config {
    fn to_yaml(&self, prefix: &str) -> String {
        let mut output = self.migrate.to_yaml(prefix);
        if let Some(ref balena) = self.balena {
            output += &balena.to_yaml(prefix);
        }
        #[cfg(debug_assertions)]
        self.print_debug_config(prefix, &mut output);

        output
    }

    fn from_yaml(&mut self, yaml: &Yaml) -> Result<(), MigError> {
        if let Some(ref section) = get_yaml_val(yaml, &["migrate"])? {
            self.migrate.from_yaml(section)?;
        }

        if let Some(section) = get_yaml_val(yaml, &["balena"])? {
            // Params: balena_image
            if let Some(ref mut balena) = self.balena {
                balena.from_yaml(section)?;
            } else {
                let mut balena = BalenaConfig::default();
                balena.from_yaml(section)?;
                self.balena = Some(balena);
            }
        }

        #[cfg(debug_assertions)]
        self.get_debug_config(yaml)?;

        Ok(())
    }
}

pub fn get_yaml_val<'a>(doc: &'a Yaml, path: &[&str]) -> Result<Option<&'a Yaml>, MigError> {
    debug!("{}::get_yaml_val: looking for '{:?}'", MODULE, path);
    let mut last = doc;

    for comp in path {
        debug!("{}::get_yaml_val: looking for comp: '{}'", MODULE, comp);
        match last {
            Yaml::Hash(_v) => {
                let curr = &last[*comp];
                if let Yaml::BadValue = curr {
                    debug!(
                        "{}::get_yaml_val: not found, comp: '{}' in {:?}",
                        MODULE, comp, last
                    );
                    return Ok(None);
                }
                last = &curr;
            }
            _ => {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::get_yaml_val: invalid value in path, not hash for {:?}",
                        MODULE, path
                    ),
                ));
            }
        }
    }

    Ok(Some(&last))
}

pub fn get_yaml_bool<'a>(doc: &'a Yaml, path: &[&str]) -> Result<Option<bool>, MigError> {
    debug!("{}::get_yaml_bool: looking for '{:?}'", MODULE, path);
    if let Some(value) = get_yaml_val(doc, path)? {
        match value {
            Yaml::Boolean(b) => {
                debug!(
                    "{}::get_yaml_bool: looking for comp: {:?}, got {}",
                    MODULE, path, b
                );
                Ok(Some(*b))
            }
            _ => Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::get_yaml_bool: invalid value, not bool for {:?}",
                    MODULE, path
                ),
            )),
        }
    } else {
        Ok(None)
    }
}

pub fn get_yaml_int<'a>(doc: &'a Yaml, path: &[&str]) -> Result<Option<i64>, MigError> {
    debug!("{}::get_yaml_int: looking for '{:?}'", MODULE, path);
    if let Some(value) = get_yaml_val(doc, path)? {
        match value {
            Yaml::Integer(i) => {
                debug!(
                    "{}::get_yaml_int: looking for comp: {:?}, got {}",
                    MODULE, path, i
                );
                Ok(Some(*i))
            }
            _ => Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::get_yaml_int: invalid value, not int for {:?}",
                    MODULE, path
                ),
            )),
        }
    } else {
        Ok(None)
    }
}

pub fn get_yaml_str<'a>(doc: &'a Yaml, path: &[&str]) -> Result<Option<&'a str>, MigError> {
    debug!("{}::get_yaml_str: looking for '{:?}'", MODULE, path);
    if let Some(value) = get_yaml_val(doc, path)? {
        match value {
            Yaml::String(s) => {
                debug!(
                    "{}::get_yaml_str: looking for comp: {:?}, got {}",
                    MODULE, path, s
                );
                Ok(Some(&s))
            }
            _ => Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!(
                    "{}::get_yaml_str: invalid value, not string for {:?}",
                    MODULE, path
                ),
            )),
        }
    } else {
        Ok(None)
    }
}

/*
fn get_yaml_str_def<'a>(doc: &'a Yaml, path: &[&str], default: &'a str) -> Result<&'a str,MigError> {
    debug!("{}::get_yaml_str_def: looking for '{:?}', default: '{}'", MODULE, path, default);
    if let Some(value) = get_yaml_val(doc, path)? {
        match value {
            Yaml::String(s) => {
                debug!("{}::get_yaml_str_def: looking for comp: {:?}, got {}", MODULE, path, s );
                Ok(&s)
                },
            _ => Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::get_yaml_str_def: invalid value, not string for {:?}", MODULE, path)))
        }
    } else {
        Ok(default)
    }
}
*/

#[cfg(test)]
mod tests {
    use super::*;

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
            assert!(balena.image == "image.gz");
            assert!(balena.config == "config.json");
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

}
