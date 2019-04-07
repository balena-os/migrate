use clap::{ArgMatches};
use log::{info, debug};
use yaml_rust::{YamlLoader, Yaml};
use std::fs::read_to_string;
use failure::ResultExt;

use crate::migrator::{ 
    MigError,
    MigErrorKind,
    MigErrCtx,
};

pub mod log_config;
pub use log_config::LogConfig;

pub mod migrate_config;
pub use migrate_config::{MigrateConfig, MigMode};

pub mod balena_config;
pub use balena_config::{BalenaConfig};

#[cfg(debug_assertions)]
pub mod debug_config;
#[cfg(debug_assertions)]
pub use debug_config::{DebugConfig};


/*

LOG_DRIVE= # /dev/sdb1
LOG_FS_TYPE=ext4
Ü
################################################################################
# where everything is
HOME_DIR=./

################################################################################
# reboot automatically after script has finished by setting to number of seconds 
# before rebboot
DO_REBOOT= # 10

################################################################################
# name of the balenaOS image to flash (expected in $HOMEDIR)
# must be set in /etc/balena-migrate.conf
# IMAGE_NAME="resin-image-genericx86-64.resinos-img.gz"
# IMAGE_NAME="resin-resintest-raspberrypi3-2.15.1+rev2-dev-v7.16.6.img.gz"


################################################################################
# create NM configs from all configs found in this system
MIGRATE_ALL_WIFIS="FALSE" # migrate all wifis if set to "TRUE"

################################################################################
# only create NM wifi configs for ssids listed in this file
# file with a list of wifi networks to migrate, one per line
MIGRATE_WIFI_CFG="migrate-wifis"

################################################################################
# inject the config.json provided under the given filename into resin-boot
# set to the path of a config.json file to copy to the image
BALENA_CONFIG=

################################################################################
# if set to TRUE attempt to extract a wifi config from config.json given in
# BALENA_CONFIG
BALENA_WIFI=

################################################################################
# switch on initramfs / kernel debug mode by seting to "TRUE"
DEBUG= # "TRUE"

################################################################################
# customer defined backup script to call
BACKUP_SCRIPT=

################################################################################
# Backup definition file
BACKUP_DEFINITION=

################################################################################
# Grub boot device in grub notation - usually not nee
GRUB_BOOT_DEV="hd0"

################################################################################
# minimum free memory in stage 2
# stage 2 script reads free memory
#   subtracts size of image file & backup files
#   fails if remaining space is less than the value given in MEM_MIN_FREE
MEM_MIN_FREE_S2=65536   # 64 MB as kB

################################################################################
# minimum free memory in stage 1
# stage 1 script reads total memory
#   subtracts size of image file & backup files & initramfs
#   fails if remaining space is less than the value given in MEM_MIN_FREE
MEM_MIN_FREE_S1=65536   # 64 MB as kB

################################################################################
# DEBUG end initramfs scripts before unmounting root / flashing the image
NO_FLASH= #"TRUE"

################################################################################
# DEBUG: do not modify config.txt, cmdline.txt, grub config if set to "TRUE"
NO_SETUP= #{ }"TRUE"

################################################################################
# Test connectivity to API and VPN hosts"
BALENA_API_HOST="api.balena-cloud.com"
BALENA_API_PORT=443
BALENA_VPN_HOST="vpn.balena-cloud.com"
BALENA_VPN_PORT=443
BALENA_CONNECT_TIMEOUT=20

################################################################################
# Fail if no network manager file is created
REQUIRE_NMGR_FILE=TRUE

################################################################################
# DEBUG verbose build process
MK_INITRAM_VERBOSE= # "TRUE"

################################################################################
# DEBUG keep initramfs layout
MK_INITRAM_RETAIN= # "TRUE"

################################################################################
# Minimum required free diskspace in KB, default 10MB
MIN_ROOT_AVAIL=10240 

################################################################################
# DEBUG enable DEBUG messages if set to TRUE
LOG_DEBUG= # "TRUE"
DEBUG_FUNCTS="main setupBootCfg_bb clean bbg_setup"

################################################################################
# Max acceptable bad blocks - a scan is only performed if value is set
MAX_BADBLOCKS=

################################################################################
# Min acceptable device write speed
MIN_WRITE_SPEED=
WARN_WRITE_SPEED=200
*/

const MODULE: &str = "migrator::common::config";

// TODO: add trait ToYaml and implement for all sections

pub trait YamlConfig {
    fn to_yaml(&self, prefix: &str) -> String;
    fn from_yaml(&mut self, yaml: & Yaml) -> Result<(),MigError>;    
}

#[derive(Debug)]
pub struct Config {
    pub migrate: MigrateConfig,
    pub balena: Option<BalenaConfig>,
#[cfg(debug_assertions)]
    pub debug: DebugConfig,
}


impl<'a> Config {    
    pub fn new(arg_matches: &ArgMatches) -> Result<Config, MigError> {
        // defaults to 
        let mut config = Config::default();

        if arg_matches.is_present("config") {
            config.from_file(arg_matches.value_of("config").unwrap())?;
        }

        if arg_matches.is_present("immediate") {
            config.migrate.mode = MigMode::IMMEDIATE;
        } else if arg_matches.is_present("agent") {
            config.migrate.mode = MigMode::AGENT;
        }

        info!("{}::new: migrate mode: {:?}",MODULE, config.migrate.mode);

        debug!("{}::new: got: {:?}", MODULE, config);

        config.check()?;

        Ok(config)
    }

    fn default() -> Config {
        Config{ 
            migrate: MigrateConfig::default(),
            balena: None,            
#[cfg(debug_assertions)]
            debug: DebugConfig::default(),
        }
    }

#[cfg(debug_assertions)]
    fn get_debug_config(&mut self, yaml: &Yaml) -> Result<(),MigError> {
        if let Some(section) = get_yaml_val(yaml, &["debug"])? {
            self.debug.from_yaml(section)?
        }           
        Ok(())
    }

#[cfg(debug_assertions)]
    fn print_debug_config(&self, prefix: &str, buffer: &mut String ) -> () {
        *buffer += &self.debug.to_yaml(prefix)
    }



    fn from_string(&mut self, config_str: &str) -> Result<(),MigError> {
        debug!("{}::from_string: entered", MODULE);
        let yaml_cfg = YamlLoader::load_from_str(&config_str).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::from_string: failed to parse", MODULE)))?;
        if yaml_cfg.len() != 1 {
            return Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::from_string: invalid number of configs in file: {}", MODULE, yaml_cfg.len())));
        }
        
        self.from_yaml(&yaml_cfg[0])
    }


    fn from_file(&mut self, file_name: &str) -> Result<(),MigError> {
        debug!("{}::from_file: {} entered", MODULE, file_name);

        self.from_string(&read_to_string(file_name).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::from_file: failed to read {}", MODULE, file_name)))?)
    }

    fn check(&self) -> Result<(),MigError> {
        match self.migrate.mode {
            MigMode::AGENT => {
            },
            MigMode::IMMEDIATE => {
                if let Some(balena) = &self.balena {
                    balena.check(&self.migrate.mode)?;
                } else {
                    return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::check: no balena section was specified in mode: IMMEDIATE", MODULE)));
                }              
            },
            MigMode::INVALID => { return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::check: no migrate mode was selected", MODULE))); },                
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

    fn from_yaml(&mut self, yaml: & Yaml) -> Result<(),MigError> {
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


fn get_yaml_val<'a>(doc: &'a Yaml, path: &[&str]) -> Result<Option<&'a Yaml>,MigError> {
    debug!("{}::get_yaml_val: looking for '{:?}'", MODULE, path);
    let mut last = doc;

    for comp in path {
        debug!("{}::get_yaml_val: looking for comp: '{}'", MODULE, comp );
        match last {
            Yaml::Hash(_v) => {                
                let curr = &last[*comp];
                if let Yaml::BadValue = curr {
                    debug!("{}::get_yaml_val: not found, comp: '{}' in {:?}", MODULE, comp, last );
                    return Ok(None)
                }
                last = &curr;
            },
            _ => {
                return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::get_yaml_val: invalid value in path, not hash for {:?}", MODULE, path)));
            }
        }        
    }

    Ok(Some(&last))
}

fn get_yaml_bool<'a>(doc: &'a Yaml, path: &[&str]) -> Result<Option<bool>,MigError> {
    debug!("{}::get_yaml_bool: looking for '{:?}'", MODULE, path);
    if let Some(value) = get_yaml_val(doc, path)? {
        match value {
            Yaml::Boolean(b) => { 
                debug!("{}::get_yaml_bool: looking for comp: {:?}, got {}", MODULE, path, b );
                Ok(Some(*b))
                },
            _ => Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::get_yaml_bool: invalid value, not bool for {:?}", MODULE, path)))
        }
    } else {
        Ok(None)
    }
}


fn get_yaml_int<'a>(doc: &'a Yaml, path: &[&str]) -> Result<Option<i64>,MigError> {
    debug!("{}::get_yaml_int: looking for '{:?}'", MODULE, path);
    if let Some(value) = get_yaml_val(doc, path)? {
        match value {
            Yaml::Integer(i) => { 
                debug!("{}::get_yaml_int: looking for comp: {:?}, got {}", MODULE, path, i );
                Ok(Some(*i))
                },
            _ => Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::get_yaml_int: invalid value, not int for {:?}", MODULE, path)))
        }
    } else {
        Ok(None)
    }
}

fn get_yaml_str<'a>(doc: &'a Yaml, path: &[&str]) -> Result<Option<&'a str>,MigError> {
    debug!("{}::get_yaml_str: looking for '{:?}'", MODULE, path);
    if let Some(value) = get_yaml_val(doc, path)? {
        match value {
            Yaml::String(s) => { 
                debug!("{}::get_yaml_str: looking for comp: {:?}, got {}", MODULE, path, s );
                Ok(Some(&s))
                },
            _ => Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::get_yaml_str: invalid value, not string for {:?}", MODULE, path)))
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
            _ => { panic!("unexpected migrate mode"); }
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
