use super::{MigError, MigErrorKind};
use log::debug;
use serde_json::{value::Index, Value};
use yaml_rust::Yaml;

const MODULE: &str = "migrator::common::config_helper";

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

pub fn get_json_str<'a, I: Index>(doc: &'a Value, index: I) -> Result<Option<&'a str>, MigError> {
    if let Some(value) = doc.get(index) {
        match value {
            Value::String(s) => Ok(Some(&s)),
            _ => Err(MigError::from_remark(
                MigErrorKind::InvParam,
                &format!("{}::get_json_str: invalid value, not string", MODULE),
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
