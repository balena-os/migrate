use clap::{ArgMatches};
use log::{info, debug};
use yaml_rust::{YamlLoader, Yaml};
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
const DEFAULT_MODE: MigMode = MigMode::INVALID;

// TODO: add trait ToYaml and implement for all sections

#[derive(Debug)]
pub struct LogConfig {
    pub drive: String,
    pub fs_type: String,
}

impl LogConfig {
    fn to_yaml(&self, prefix: &str) -> String {
        format!(
            "{}log_to:\n{}  drive: '{}'\n{}  fs_type: '{}'\n", prefix, prefix, self.drive, prefix , self.fs_type)
    }
}


#[derive(Debug)]
pub struct MigrateConfig {
    pub mode: MigMode,
    pub reboot: Option<u64>,
    pub all_wifis: bool,
    pub log_to: Option<LogConfig>,
} 

impl MigrateConfig {
    fn to_yaml(&self, prefix: &str) -> String {
        let mut output = format!("{}migrate:\n{}  mode: '{:?}'\n{}  all_wifis: {}\n", prefix, prefix, self.mode, prefix, self.all_wifis);
        if let Some(i) = self.reboot {
            output += &format!("{}  reboot: {}\n", prefix, i);
        }

        let next_prefix = String::from(prefix) + "  ";        
        if let Some(ref log_to) = self.log_to {
            output += &log_to.to_yaml(&next_prefix);
        }

        output
    }
}


#[derive(Debug)]
pub struct BalenaConfig {
    pub image: String,
    pub config: String,
} 

impl BalenaConfig {
    fn default() -> BalenaConfig {
        BalenaConfig{
            image: String::from(""),
            config: String::from(""),
        }
    }

    fn check(&self, mig_mode: &MigMode) -> Result<(),MigError> {
        if let MigMode::IMMEDIATE = mig_mode {
            if self.image.is_empty() {
                return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::check: no balena OS image was specified in mode: IMMEDIATE", MODULE)));
            }                

            if self.config.is_empty() {
                return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::check: no config.json was specified in mode: IMMEDIATE", MODULE)));
            }  
        }

        Ok(())
    }

    fn to_yaml(&self, prefix: &str) -> String {
        format!(
            "{}balena:\n{}  image: '{}'\n{}  config: '{}'\n", prefix, prefix, self.image, prefix , self.config)
    }
}

#[derive(Debug)]
pub struct Config {
    pub migrate: MigrateConfig,
    pub balena: Option<BalenaConfig>,
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
            migrate: MigrateConfig{
                mode: DEFAULT_MODE,
                reboot: None,
                all_wifis: false,
                log_to: None,
            },
            balena: None,            
        }
    }

    fn from_string(&mut self, config_str: &str) -> Result<(),MigError> {
        debug!("{}::from_string: entered", MODULE);
        let yaml_cfg = YamlLoader::load_from_str(&config_str).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("{}::from_string: failed to parse", MODULE)))?;
        if yaml_cfg.len() != 1 {
            return Err(MigError::from_remark(MigErrorKind::InvParam,&format!("{}::from_string: invalid number of configs in file: {}", MODULE, yaml_cfg.len())));
        }
        
        let yaml_cfg = &yaml_cfg[0];
        
        
        // Section Migrate:
        if let Some(section) = get_yaml_val(yaml_cfg, &["migrate"])? {
            // Params: mode
            if let Some(mode) = get_yaml_str(section, &["mode"])? {
                if mode.to_lowercase() == "immediate" {
                    self.migrate.mode = MigMode::IMMEDIATE;
                } else if mode.to_lowercase() == "agent" {
                    self.migrate.mode = MigMode::AGENT;
                } else {
                    return Err(MigError::from_remark(MigErrorKind::InvParam, &format!("{}::from_string: invalid value for migrate mode '{}'", MODULE, mode)));
                }            
            }

            // Param: reboot - must be > 0 
            if let Some(reboot_timeout) = get_yaml_int(section, &["reboot"])? {
                if reboot_timeout > 0 {
                    self.migrate.reboot = Some(reboot_timeout as u64);      
                } else {
                    self.migrate.reboot = None;      
                }
            }

            // Param: all_wifis - must be > 0 
            if let Some(all_wifis) = get_yaml_bool(section, &["all_wifis"])? {
                self.migrate.all_wifis = all_wifis;      
            }

            // Params: log_to: drive, fs_type 
            if let Some(log_section) = get_yaml_val(section, &["log_to"])? {
                if let Some(log_drive) = get_yaml_str(log_section, &["drive"])? {
                    if let Some(log_fs_type) = get_yaml_str(log_section, &["fs_type"])? {
                        self.migrate.log_to = Some(
                            LogConfig{
                                drive: String::from(log_drive),
                                fs_type: String::from(log_fs_type),
                        });
                    }    
                }
            }
        }

        if let Some(section) = get_yaml_val(yaml_cfg, &["balena"])? {
            // Params: balena_image 
            let mut balena = BalenaConfig::default();
            if let Some(balena_image) = get_yaml_str(section, &["image"])? {
                balena.image = String::from(balena_image);
            }

            // Params: balena_config 
            if let Some(balena_config) = get_yaml_str(section, &["config"])? {
                balena.config = String::from(balena_config);                
            }

            self.balena = Some(balena);
        }

        // TODO: Eval yaml

        Ok(())

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

    pub fn to_yaml(&self) -> String {
        let mut output = self.migrate.to_yaml("");
        if let Some(ref balena) = self.balena {
            output += &balena.to_yaml("");
        }
        output
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
        
        let out = config.to_yaml();
        
        let mut new_config = Config::default();
        new_config.from_string(&out).unwrap(); 
        assert_test_config1(&new_config);           

        ()
    }

}
