use clap::{ArgMatches};
use log::{info};
use yaml_rust::{YamlLoader};
use std::fs::read_to_string;
use failure::ResultExt;

#[derive(Debug)]
pub enum MigMode {
    INVALID,
    AGENT,
    IMMEDIATE,
}

use crate::migrator::{ 
    MigError,
    MigErrorKind,
    MigErrCtx,
};

#[derive(Debug)]
pub struct Config {
    pub mode: MigMode,
}

const MODULE: &str = "migrator::common::config";

const DEFAULT_MODE: MigMode = MigMode::INVALID;

impl Config {
    pub fn new(arg_matches: &ArgMatches) -> Result<Config, MigError> {
        // defaults to 
        let mut config = Config{ mode: DEFAULT_MODE };

        let mig_mode = DEFAULT_MODE;
        if arg_matches.is_present("immediate") {
            config.mode = MigMode::IMMEDIATE;
        } else if arg_matches.is_present("agent") {
            config.mode = MigMode::AGENT;
        }

        info!("{}::new: migrate mode: {:?}",MODULE, config.mode);

        if arg_matches.is_present("config") {
            config.from_file(arg_matches.value_of("config").unwrap())?;
        }

        config.check()?;

        Ok(config)
    }

    fn from_file(&mut self, file_name: &str) -> Result<(),MigError> {
        let config_str = read_to_string(file_name).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::from_file: failed to read {}", MODULE, file_name)))?;        
        let yaml_cfg = YamlLoader::load_from_str(&config_str).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::from_file: failed to parse {}", MODULE, file_name)))?;

        // TODO: Eval yaml

        Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn check(&self) -> Result<(),MigError> {
        if let MigMode::INVALID = self.mode {
            return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::check: no migrate mode was selected", MODULE)));
        }

        Err(MigError::from(MigErrorKind::NotImpl))
    }
}