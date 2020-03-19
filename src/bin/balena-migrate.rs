// executable wrapper for balena-migrate

use balena_migrate::{
    common::{assets::Assets, MigErrorKind},
    migrate,
};

fn init_assets() -> Assets {
    if cfg!(feature = "raspberrypi3") {
        let mut assets = Assets {
            asset_type: String::from("raspberrypi3"),
            kernel: include_bytes!("../../balena-boot/raspberrypi3/balena-migrate.zImage"),
            initramfs: include_bytes!("../../balena-boot/raspberrypi3//balena.initrd.cpio.gz"),
            /*stage2: include_bytes!(
                "../../target/armv7-unknown-linux-musleabihf/release/balena-stage2"
            ), */
            dtbs: Vec::new(),
        };
        assets.dtbs.push((
            String::from("bcm2710-rpi-3-b.dtb"),
            include_bytes!("../../balena-boot/raspberrypi3/bcm2710-rpi-3-b.dtb"),
        ));

        assets.dtbs.push((
            String::from("bcm2710-rpi-3-b-plus.dtb"),
            include_bytes!("../../balena-boot/raspberrypi3/bcm2710-rpi-3-b-plus.dtb"),
        ));

        assets
    } else if cfg!(feature = "raspberrypi4-64") {
        Assets {
            asset_type: String::from("raspberrypi4-64"),
            kernel: include_bytes!("../../balena-boot/raspberrypi4-64/balena-migrate.zImage"),
            initramfs: include_bytes!("../../balena-boot/raspberrypi4-64/balena.initrd.cpio.gz"),
            // stage2: include_bytes!("../../target/aarch64-unknown-linux-musl/release/balena-stage2"),
            dtbs: Vec::new(),
        }
    } else {
        panic!("No assets included")
    }
}

fn main() {
    if let Err(error) = migrate(&init_assets()) {
        match error.kind() {
            MigErrorKind::Displayed => {
                println!("balena-migrate failed with an error, see messages above");
            }
            _ => {
                println!("balena-migrate failed with an error: {}", error);
            }
        }
    }
}
