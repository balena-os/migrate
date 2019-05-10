use log::{error, info, trace, warn};
use failure::{ResultExt, Fail};
use std::fs::{copy, File};
use std::io::{BufReader, BufRead, Write};
use std::path::Path;
use std::time::{SystemTime};
use regex::{Regex, Captures};


use crate::{
    common::{Config, MigError, MigErrorKind, MigErrCtx, path_append, file_exists, is_balena_file},
    linux_common::{Device, migrate_info::MigrateInfo},
    stage2::Stage2Config,
    defs::{BALENA_FILE_TAG },
};

const RPI_MODEL_REGEX: &str = r#"^Raspberry\s+Pi\s+(\S+)\s+Model\s+(.*)$"#;
const RPI_CONFIG_TXT: &str = "config.txt";

pub(crate) fn is_rpi(model_string: &str) -> Result<Box<Device>, MigError> {
    trace!(
        "Beaglebone::is_bb: entered with model string: '{}'",
        model_string
    );

    if let Some(captures) = Regex::new(RPI_MODEL_REGEX).unwrap().captures(model_string) {
        let pitype = captures.get(1).unwrap().as_str();
        let model = captures
            .get(2)
            .unwrap()
            .as_str()
            .trim_matches(char::from(0));

        match pitype {
            "3" => {
                info!("Identified RaspberryPi3: model {}", model);
                Ok(Box::new(RaspberryPi3 {}))
            }
            _ => {
                let message = format!("The raspberry pi type reported by your device ('{} {}') is not supported by balena-migrate", pitype, model);
                error!("{}", message);
                Err(MigError::from_remark(MigErrorKind::InvParam, &message))
            }
        }
    } else {
        warn!("no match for Raspberry PI on: {}", model_string);
        Err(MigError::from(MigErrorKind::NoMatch))
    }
}

pub(crate) struct RaspberryPi3 {}

impl RaspberryPi3 {
    pub(crate) fn new() -> RaspberryPi3 {
        RaspberryPi3 {}
    }
}

impl<'a> Device for RaspberryPi3 {
    fn get_device_slug(&self) -> &'static str {
        "raspberrypi3"
    }

    fn setup(&self, _config: &Config, mig_info: &mut MigrateInfo) -> Result<(), MigError> {
        trace!(
            "RaspberryPi3::setup: entered with type: '{}'",
            match &mig_info.device_slug {
                Some(s) => s,
                _ => panic!("no device type slug found"),
            }
        );

        let boot_path = mig_info.get_boot_path();
        let config_path = path_append(boot_path, RPI_CONFIG_TXT);

        if !file_exists(&config_path) {
            return Err(MigError::from_remark(MigErrorKind::NotFound, &format!("Could not find '{}'", config_path.display())));
        }

        // create backup of config.txt

        if ! is_balena_file(&config_path)? {
            let system_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).context(MigErrCtx::from_remark(MigErrorKind::Upstream, "Failed to create timestamp"))?;
            let backup_path = path_append(boot_path, &format!("{}.{}", RPI_CONFIG_TXT, system_time.as_secs()));

            copy(&config_path, &backup_path).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed to copy '{}' to '{}'", config_path.display(), backup_path.display())))?;
            mig_info.boot_cfg_bckup.push((String::from(&*config_path.to_string_lossy()), String::from(&*backup_path.to_string_lossy())));
            info!("Created backup of '{}' in '{}'", config_path.display(), backup_path.display());
        } else {
            // TODO: what to do if it is a balena-migrate created config.txt ?
        }

        let initramfs_re = Regex::new(r#"^\s*initramfs"#).unwrap();

        let mut out_str = format!("{}\n", BALENA_FILE_TAG);
        let mut found = false;

        {
            let config_file = File::open(&config_path).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed to open file '{}'", config_path.display())))?;
            for line in BufReader::new(config_file).lines() {
                match line {
                    Ok(line) => {
                        if initramfs_re.is_match(&line) {
                            if found {
                                // one initrd comand is enough
                                out_str.push_str(&format!("# {}\n", line));
                            } else {
                                out_str.push_str(&format!("initramfs {} followkernel\n", &mig_info.get_initrd_path().to_string_lossy()));
                                found = true;
                            }
                        } else {
                            out_str.push_str(&format!("{}\n", &line));
                        }
                    },
                    Err(why) => {
                        return Err(MigError::from(why.context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed to read line from file '{}'", config_path.display())))));
                    }
                }
            }
        }
        if !found {
            // add it if it did not exist
            out_str.push_str(&format!("initramfs {} followkernel\n", &mig_info.get_initrd_path().to_string_lossy()));
        }

        let mut config_file = File::create(&config_path).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed to open file '{}' for writing", config_path.display())))?;
        config_file.write(out_str.as_bytes()).context(MigErrCtx::from_remark(MigErrorKind::Upstream, &format!("Failed write to file '{}'", config_path.display())))?;






        /*
                ####################################################
                ## setup raspbery pi to boot using initramfs
                ####################################################

                function setupBootCfg_rpi {
                    if [ "$NO_SETUP" != "TRUE" ] ; then
                # RESTORE_BOOT="TRUE"

                CONFIG_TXT_BACKUP="config.txt.$(date +%Y%m%d-%H-%M-%S)"

                cp "${S1_BOOT_PATH}/config.txt" "${S1_BOOT_PATH}/${CONFIG_TXT_BACKUP}"
                inform "created backup of ${S1_BOOT_PATH}/${CONFIG_TXT} in ${S1_BOOT_PATH}/${CONFIG_TXT_BACKUP}"
                RESTORE_BOOT_CFG_STAGE1="mv ${S1_BOOT_PATH}/${CONFIG_TXT_BACKUP} ${S1_BOOT_PATH}/config.txt"
                RESTORE_BOOT_CFG_STAGE2="cp ${S2_BOOT_PATH}/${CONFIG_TXT_BACKUP} ${S2_BOOT_PATH}/config.txt"

                TMP_FILE=$(mktemp)
                INITRAM_CMD="initramfs ${INITRAMFS_NAME} followkernel"

                while read -r line
                do
                if [[ $line =~ ^\ *\# ]] ; then
                echo "$line" >> "$TMP_FILE"
                continue
                fi

                if [[ $line =~ ^\ *initramfs ]] ; then
                if [ "$line" != "$INITRAM_CMD" ] ; then
                echo "# $line" >> "$TMP_FILE"
                fi
                else
                echo "$line" >> "$TMP_FILE"
                fi
                done < "${CONFIG_TXT}"
                echo "$INITRAM_CMD" >> "$TMP_FILE"
                cp "$TMP_FILE" "$CONFIG_TXT"
                rm "$TMP_FILE"

                ###########################
                ## Modify /boot/cmdline.txt
                ###########################

                if [ "$DEBUG" == "TRUE" ] ; then
                CMD_LINE=$(cat "${S1_BOOT_PATH}/cmdline.txt")
                if [[ ! $CMD_LINE =~ debug ]] ; then
                CMDLINE_TXT_BACKUP="cmdline.txt.$(date +%Y%m%d-%H-%M-%S)"
                inform "creating backup of ${CMDLINE_TXT} in ${CMDLINE_TXT_BACKUP}"
                cp "${S1_BOOT_PATH}/cmdline.txt"  "${S1_BOOT_PATH}/${CMDLINE_TXT_BACKUP}"
                CMD_LINE="${CMD_LINE} debug"
                echo "$CMD_LINE" > "${S1_BOOT_PATH}/cmdline.txt"

                RESTORE_BOOT_CFG_STAGE1="${RESTORE_BOOT_CFG_STAGE1} && mv ${S1_BOOT_PATH}/${CMDLINE_TXT_BACKUP} ${S1_BOOT_PATH}/cmdline.txt"
                RESTORE_BOOT_CFG_STAGE2="${RESTORE_BOOT_CFG_STAGE2} && cp ${S2_BOOT_PATH}/${CMDLINE_TXT_BACKUP} ${S2_BOOT_PATH}/cmdline.txt"
                fi
                fi

                if [ -n "${RESTORE_BOOT_CFG_STAGE2}" ] ; then
                RESTORE_BOOT_CFG_STAGE2="\"${RESTORE_BOOT_CFG_STAGE2}\""
                fi

                debug setupBootCfg_rpi "RESTORE_BOOT_CFG_STAGE1=${RESTORE_BOOT_CFG_STAGE1}"
                debug setupBootCfg_rpi "RESTORE_BOOT_CFG_STAGE2=${RESTORE_BOOT_CFG_STAGE2}"

                else
                inform "boot setup is disabled, NO_SETUP=$NO_SETUP"
                fi
            }
         */

        Err(MigError::from(MigErrorKind::NotImpl))
    }

    fn restore_boot(&self, _root_path: &Path, _config: &Stage2Config) -> Result<(), MigError> {
        Err(MigError::from(MigErrorKind::NotImpl))
    }
}
