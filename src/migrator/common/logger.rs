use chrono::Local;
use colored::*;
use failure::ResultExt;
use log::{debug, Level, Log, Metadata, Record};
use regex::{Captures, Regex};
use std::collections::HashMap;
use std::env;
use std::fs::read_to_string;
use std::str::FromStr;
use yaml_rust::{Yaml, YamlLoader};

use super::config::{get_yaml_str, get_yaml_val};
use crate::migrator::{MigErrCtx, MigError, MigErrorKind};

const MODULE: &str = "migrator::common::logger";

pub const DEFAULT_LOG_LEVEL: Level = Level::Warn;

#[derive(Debug)]
pub struct Logger {
    default_level: Level,
    mod_level: HashMap<String, Level>,
    module_re: Regex,
}

impl Logger {
    pub fn initialise(default_log_level: usize) -> Result<(), MigError> {
        // config:  &Option<LogConfig>)

        let mut logger = Logger {
            default_level: DEFAULT_LOG_LEVEL,
            mod_level: HashMap::new(),
            module_re: Regex::new(r#"^[^:]+::(.*)$"#).unwrap(),
        };

        let mut max_level = logger.default_level;

        if let Ok(config_path) = env::var("LOG_CONFIG") {
            let config_str = &read_to_string(&config_path).context(MigErrCtx::from_remark(
                MigErrorKind::Upstream,
                &format!("{}::from_file: failed to read {}", MODULE, config_path),
            ))?;
            let yaml_cfg =
                YamlLoader::load_from_str(&config_str).context(MigErrCtx::from_remark(
                    MigErrorKind::Upstream,
                    &format!("{}::from_string: failed to parse", MODULE),
                ))?;
            if yaml_cfg.len() > 1 {
                return Err(MigError::from_remark(
                    MigErrorKind::InvParam,
                    &format!(
                        "{}::from_string: invalid number of configs in file: {}, {}",
                        MODULE,
                        config_path,
                        yaml_cfg.len()
                    ),
                ));
            }

            if yaml_cfg.len() == 1 {
                let yaml_cfg = &yaml_cfg[0];
                if let Some(level) = get_yaml_str(yaml_cfg, &["log_level"])? {
                    if let Ok(level) = Level::from_str(level.as_ref()) {
                        logger.default_level = level;
                    }
                }

                if let Some(modules) = get_yaml_val(yaml_cfg, &["modules"])? {
                    if let Yaml::Array(ref modules) = modules {
                        for module in modules {
                            if let Some(name) = get_yaml_str(module, &["name"])? {
                                if let Some(level_str) = get_yaml_str(module, &["level"])? {
                                    if let Ok(level) = Level::from_str(level_str.as_ref()) {
                                        // println!("{}::initialise: adding {} : {}", MODULE, name, level);
                                        logger.mod_level.insert(String::from(name), level);
                                        if level > max_level {
                                            max_level = level;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(level) = Logger::level_from_usize(default_log_level) {
            logger.default_level = level;
        }

        if logger.default_level > max_level {
            max_level = logger.default_level;
        }

        log::set_boxed_logger(Box::new(logger)).context(MigErrCtx::from_remark(
            MigErrorKind::Upstream,
            &format!("{}::initialise: failed to initialize logger", MODULE),
        ))?;
        log::set_max_level(max_level.to_level_filter());

        Ok(())
    }

    // TODO: not my favorite solution but the corresponding level function is private
    fn level_from_usize(level: usize) -> Option<Level> {
        match level {
            0 => None,
            1 => Some(Level::Info),
            2 => Some(Level::Debug),
            _ => Some(Level::Trace),
        }
    }
}

impl Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        let mut level = self.default_level;

        let mut mod_name = String::from("undefined");
        if let Some(mod_path) = record.module_path() {
            if let Some(ref captures) = self.module_re.captures(mod_path) {
                mod_name = String::from(captures.get(1).unwrap().as_str());
            }
        }

        if let Some(mod_level) = self.mod_level.get(&mod_name) {
            level = *mod_level;
        }

        let curr_level = record.metadata().level();
        if curr_level <= level {
            let output = format!(
                "{} {:<5} [{}] {}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level().to_string(),
                &mod_name,
                record.args()
            );

            match curr_level {
                Level::Error => println!("{}", output.red()),
                Level::Warn => println!("{}", output.yellow()),
                Level::Info => println!("{}", output.green()),
                Level::Debug => println!("{}", output.cyan()),
                Level::Trace => println!("{}", output.blue()),
            };
        }
    }

    fn flush(&self) {}
}
