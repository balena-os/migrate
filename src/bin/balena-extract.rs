/*
if let MigMode::Extract = config.migrate.get_mig_mode() {
if arg_matches.is_present("device-type") {
if let Some(dev_type) = arg_matches.value_of("device-type") {
config.migrate.set_extract_device(dev_type);
}
} else {
error!("device-type option is mandatory for mode EXTRACT. Please specify a device type using the --device-type or -d option");
return Err(MigError::displayed());
}
}

            MigMode::Extract => {
                if let None = self.work_dir {
                    error!("A required parameter was not found: 'work_dir'");
                    return Err(MigError::displayed());
                }

                if let None = self.extract_device {
                    error!("A required parameter was not found: 'extract_device'");
                    return Err(MigError::displayed());
                }
                Ok(())
            }

            MigMode::Extract => {
                let mut extractor = Extractor::new(config)?;
                extractor.extract(None)?;
                Ok(())
            }

*/
//use balena_migrate::{extract};

#[cfg(target_os = "linux")]
fn main() {
    use balena_migrate::{common::MigErrorKind, extract};
    if let Err(error) = extract() {
        match error.kind() {
            MigErrorKind::Displayed => {
                println!("balena-extract failed with an error, see messages above");
            }
            _ => {
                println!("balena-extract failed with an error: {}", error);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn main() {
    println!("This program is only meant to be run on linux");
}
