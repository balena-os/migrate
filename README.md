# migrate

Migrate brownfield devices to Balena

This project is based on the ideas from https://github.com/balena-io-playground/balena-migrate and aims to enable 
migration of devices supported by Balena OS from Linux operating systems to Balena OS. 
Work is in progress to integrate further device types and operating systems. 
In particular work is in progress to allow migration of Windows devices to Balena OS.

The core functionality of the script based project in https://github.com/balena-io-playground/balena-migrate 
has been redesigned and re-implemented in rust to provide a more reliable and robust experience.

## Requirements

Balena migrate currently works on a small set of devices and linux flavors. Tested and working devices are:
- x86_64 devices, tested mainly on VirtualBox using Ubuntu flavors:
  - Ubuntu 18.04.3 LTS
  - Ubuntu 18.04.2 LTS
  - Ubuntu 16.04.2 LTS
  - Ubuntu 14.04.2 LTS
  - Ubuntu 14.04.5 LTS
  - Ubuntu 14.04.6 LTS
- Raspberry PI 3 using Raspian flavors:
  - Raspbian GNU/Linux 8 (jessie)
  - Raspbian GNU/Linux 9 (stretch)
  - Raspbian GNU/Linux 10 (buster)
- Raspberry PI 4 using Raspian flavors:
  - Raspbian GNU/Linux 8 (jessie)
  - Raspbian GNU/Linux 9 (stretch)
  - Raspbian GNU/Linux 10 (buster)
- Beaglebone Green / Black and Beagleboard XM using Debian 9 or Ubuntu flavors:
  - Ubuntu 18.04.2 LTS
  - Ubuntu 14.04.1 LTS
  - Debian GNU/Linux 9 (stretch)
    
Further device-types and operating systems will be added as required. Adding a new OS 
is usually trivial, adding a new device might require more effort.     


## How To


### Stage 1 - balena-migrate

Balena migrate is a binary executable file, that need to be executed with root privileges on the device 
that will be migrated. There are several command line parameters that can be set and the program will be looking 
for a YAML configuration file - by default in ```./balena-migrate.yml```.

Depending on the configuration ```balena-migrate``` will do one of the following depending on the ```mode``` setting:
- **pretend** - check requirements for migration but apply no changes to the system. All required settings and files need to 
be present and configured. 
- **immediate** - check requirements for migration and migrate the system immediately. All required settings and files need 
to be present and configured. 

The following modes are concepts that have been disccussed but are not implemented:
- **connected** - check requirements for migration and try to retrieve missing files from the balena cloud. 
Migrate immediately once all requirements are met. This mode is not implemented yet. 
- **agent** - connect to balena cloud and install ```balena-migrate``` as a service. 
Migration can be configured and triggered from the balena dashboard. This mode is not implemented yet.

In stage 1 ```balena-migrate``` tries to determine the running OS, device architecture, boot manager and the exact device type. 
Based on that information it decides if the device can be migrated.
For a successful migration ```balena-migrate``` needs to be able to modify the boot setup and boot into a balena kernel 
and initramfs. The files needed are device dependent - usualy a kernel image, an initramfs that contains stage2 executable 
of ```balena-stage2``` and possibly one or more device tree blob files. These files currently have to be provided as part 
of the configuration. 

#### Providing the Stage 2 Boot Configuration

The current version of the migrator provides pre build kernel and initramfs files as well as DTB files for all supported 
devices. The script ```script/mk_mig_config``` can be used to create a basic migrator config. The script will copy the kernel 
and other necessary files to the target directory. It expects to be run from within the migrator project directory with 
a successful build present for the target plattform. For intel-nuc and raspberrypi devices on linux a static linked **musl** build 
is required. The tools necessary for cross compiling and compiling for musl must be installed.
Due to the complex setup involved in creating cross compiled and statically linked binaries in rust the project 
currently uses the [rust-embedded cross][https://github.com/rust-embedded/cross] cross compilation tools to compile. 
The rust-embedded/cross project introduces the **cross** command that replaces the regular rust **cargo** command. 
 
For intel-nuc build:
 
```cross build --target=x86_64-unknown-linux-musl --release```

For beaglebone / beagleboard build:
 
```cross build --target=armv7-unknown-linux-gnueabihf --release```

For raspberrypi build 

``` cross build --target=armv7-unknown-linux-musleabihf --release```

Once libgcc is integrated in the migration initramfs the musl builds will be obsolete.   

```mk_mig_config``` configures a migration initramfs by unpacking the standard migrate initramfs, deleting and injecting
 initramfs scripts in init.d and adding the ```balena-stage2``` executable to the bin folder. The initramfs is then repacked
 and copied to the target folder.  

```shell script
  mk_mig_config - create a basic migration configuration
    USAGE mk_mig_config [OPTIONS]
    please run as root.
    OPTIONS:
      -h|--help                              - print this help text
      -d|--device device-slug                - use specified device slug
      -w|--work-dir path                     - use specified working directory, defaults to .
      -t|--target-dir path                   - use specified target directory, defaults to ./migrate

```

**Example:** create a configuration for raspberry pi3

```shell script
sudo ./script/mk_mig_config -d raspberrypi3 -t migrate-rpi3/
``` 


The above will create a basic configuration in ```migrate-rpi3``` that needs to be completed by supplying and configuring a balenaOS image,
and a config.json file as well as further configuration as required.

The kernel builds, dtb files and initial initramfs files are created in a yocto build. Tested versions for all 
supported devices can be found checked in to the subdirectories in ```balena_boot```. 

#### Providing the Balena Image

```balena-migrate``` also needs a balena OS image file which will be flashed to the device in stage 2. ```balena-migrate```
also currently requires a config.json file to be provided. 
Both these files can be downloaded from the dashboard. A downloaded image can usually **not** be fed to the 
migrator directly. The migrator needs to operate with as little diskspace as possible when flashing the image because 
the image will temporarily be stored in memory while the disk is being flashed. 
For this reason the migrator uses a gzip compressed image that can be streamed directly to dd rather than the zip 
compressed image that can be downloaded from the dashboard. Also for certain devices the image downloaded from the 
dashboard is a flasher image that contains the actual balena-os image. In this case the actual image needs to be extracted 
from the flasher image. 

There is a script in ```script/extract.sh``` that will extract all required files from a flasher image and save them in 
a format that the migrator can operate with. 
``` 
  extract - extract balena OS image and grub config from balena OS flasher image
    USAGE extract [OPTIONS] <image-file>
    please run as root.
    OPTIONS:
      --balena-cfg <output config.json file> - output config.json to given path
      --home <HOME_DIR used for migrate cfg> - use this directory as HOME_DIR for migrate config
      --img <output image file>              - output OS image to given path
```    

For regular images (non flasher) the input image for the migrator is created by unzipping and then gzipping the image 
downloaded from the dashboard:
```shell script
unzip balena-cloud-support1-raspberrypi3-2.31.5+rev1-v9.11.3.img.zip
gzip balena-cloud-support1-raspberrypi3-2.31.5+rev1-v9.11.3.img
```

#### Migrating Network Configuration

If configured ```balena-migrate``` will scan the device for wifi configurations and attempt to migrate them to 
NetworkManager connection files. There is plenty of room for improvement here - currently scanning network configs 
is very basic (only SSID & secret key) and supports only wifi configurations in wpa_supplicant, conmanager and 
NetworkManager format. The SSID's that are migrated are determined by two flags in ```balena-migrate.yml```. The 
```all_wifis``` flag when when set to true will attempt to migrate all wifi configurations found. The ```wifis``` flag
consists of a list of ssids. Only ssids contained in the list wil be migrated.
If no network configurations are migrated ```balena-migrate``` will refuse to migrate the device, to not create an 
offline device. This behaviour can be overridden by setting the flag ```require_nwmgr_config``` to ```false```.

Further network configuration can be supplied in NetworkManager connection files and configured using the 
```nwmgr_files```  parameter in ```balena-migrate.yml```.   

#### Flashing a device on File System Level

When migrating devices with untrustworthy SD-cards it might be worthwhile writing the image on file system level rather 
than flashing with dd. When writing on FS level the device is being partitioned and formatted by the migrator, which 
allows the use of bad block detection and mapping. The actual data is then restored from gzip archives.
Use the ```check: ro``` or ```check: rw``` option (see snippet below) to perform read or read-write (slow) checks while 
formatting.

To be able to use this feature the partitions and the partition dimensions of the balenaOS image have to be extracted 
in a separate step to migration.
This can be done using the ```balena-extract``` executable. ```balena-extract``` will extract the partition 
archives and output a configuration snippet that can be used to add the configuration to ```balena-migrate.yml``` 
```
balena-extract 0.1
Thomas Runte <thomasr@balena.io>
Extracts features from balena OS Images

USAGE:
    balena-extract [FLAGS] <image> --device-type <type>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information
    -v               Sets the level of verbosity

OPTIONS:
    -d, --device-type <type>    specify image device slug for extraction

ARGS:
    <image>    use balena OS image
```
Example invocation:

```shell script
sudo balena-extract \
     -d beaglebone-green \
     bbg/balena-cloud-bbtest-beaglebone-green-2.29.2+rev3-dev-v9.0.1.os.img.gz 

image config:
    ---
    fs:
      extended_blocks: 2162688
      device_slug: beaglebone-green
      check: ~
      max_data: ~
      mkfs_direct: ~
      boot:
        blocks: 81920
        archive:
          path: resin-boot.tgz
          hash:
            md5: 9111b8be2903683638c850c9fff047cc
      root_a:
        blocks: 638976
        archive:
          path: resin-rootA.tgz
          hash:
            md5: df0e67f5c3479ddd17f3dca9abcd74a0
      root_b:
        blocks: 638976
        archive:
          path: resin-rootB.tgz
          hash:
            md5: e03534953b5f8d867bcebf3178e44906
      state:
        blocks: 40960
        archive:
          path: resin-state.tgz
          hash:
            md5: 4cbb7304932ef21212483096a167293a
      data:
        blocks: 2105344
        archive:
          path: resin-data.tgz
          hash:
            md5: 57d78c6cfe8a6b13b283804822e0c518
``` 
The above config snippet must be added to ```balena-migrate.yml``` and the partition archives 
(```resin-xxx.tgz```) need to be present in the working directory. 
      
#### Choosing the installation device

```balena-migrate``` needs to be able to determine the installation device. Usually it will choose the device that 
contains the boot setup. Unfortunately this task is not trivial as boot partitions could potentially reside on a 
different drive from the root partition which makes it hard to detect and is generally not supported by balena OS. When 
migrating devices with more complex disk layouts and more than one OS installed, ```balena-migrate``` should be used with great 
caution and might not be the right tool at all.        

   
#### Backup Configuration

```balena-migrate``` can be configured to create a backup that will automatically be converted to volumes once 
balena-os is running on the device.
```balena-migrate.yml``` contains a section for backup configuration. The backup is grouped into volumes - volume names 
corresponding to the top level directories of the backup archive. 
Each volume can be configured to contain a complex directory structure. Volumes correspond to application container 
volumes of the application that is loaded on the device once balena OS is running. 
The balena-supervisor will scan the created backup for volumes declared in the application containers and automatically 
restore the backed up data to the appropriate container volumes. 
The supervisor will delete the backup once this process is terminated. Backup directories with no corresponding volumes 
are not retained. 

*Backup configuration example snippet:*

```yaml
backup:
   ## create a volume test volume 1
   - volume: "test volume 1"
     items:
     ## backup all from source and store in target inside the volume  
     - source: /home/thomas/develop/balena.io/support
       target: "target dir 1.1"
     - source: "/home/thomas/develop/balena.io/customer/"
       target: "target dir 1.2"
   ## create another volume 
   - volume: "test volume 2"
     items:
     ## store all files from source that match the filter in target
     - source: "/home/thomas/develop/balena.io/migrate"
       target: "target dir 2.2"
       filter: 'balena-.*'
   ## store all files from source that match the filter
   ## in the root of the volume directory
   - volume: "test_volume_3"
     items:
      - source: "/home/thomas/develop/balena.io/migrate/migratecfg/init-scripts"
        filter: 'balena-.*'
```

#### Finishing Stage 1

Once all required files are found balena-migrate will set up the device to boot into the balena kernel and initramfs, 
write a configuration file for stage 2 ```balena-stage2.yml``` and reboot the device.
The kernel is booted using a root device that contains ```balena-stage2.yml``` in the file system root. This will typically be 
the ```/boot``` partition if it is located in a separate partition or any other partition that contains the boot files 
(eg. MLO, uboot.img files for u-boot). If none of the above is available the new root will be the old root partition. 
The root partition will generally be addressed using its partuuid. 
The ```balena-stage2.yml``` will contain all necessary information to restore the former boot configuration and to mount 
and access the working directory, that contains all other required data. 

#### Example - Setting up Migration in IMMEDIATE mode 

A (working) sample configuration file:

```yaml
migrate:
  ## migrate mode
  ## 'immediate' migrate
  ## 'pretend' : just run stage 1 without modifying anything
  ## 'extract' : do not migrate extract image instead
  mode: immediate
  ## where required files are expected
  work_dir: .
  ## migrate all found wifi configurations
  all_wifis: true
  ## A list of Wifi SSID's to migrate
  # wifis:
  #   - my-ssid
  ## automatically reboot into stage 2 after n seconds
  reboot: 5
  ## stage2 log configuration
  log:
    ## use this drive for stage2 persistent logging
    drive: /dev/sda1
    ## stage2 log level (trace, debug, info, warn, error)
    level: info
  ## path to stage2 kernel - must be a balena os kernel matching the device type
  kernel: 
    path: balena.zImage
    # hash: 
    #   md5: <MD5 Hash>
  ## path to stage2 initramfs
  initrd: 
    path: balena.initramfs.cpio.gz
    # hash:
    #   md5: <MD5 Hash>
  ## path to stage2 device tree blob - better be a balena dtb matching the device type
  # device_tree: 
  # - path: balena.dtb
  #   hash:
  #     md5: <MD5 Hash>
  ## backup configuration, configured files are copied to balena and mounted as volumes
  backup:
  ## network manager configuration files
  nwmgr_files:
    # - eth0_static
  ## use internal gzip with dd true | false
  gzip_internal: ~
  ## Extra kernel commandline options
  # kernel_opts: "panic=20"
  ## Use the given device instead of the boot device to flash to
  # force_flash_device: /dev/sda
  ## delay migration by n seconds - workaround for watchdog not disabling
  # delay: 60
  ## kick / close configured watchdogs
  # watchdogs:
  ## path to watchdog device
  # - path: /dev/watchdog1
  ## optional interval in seconds - overrides interval read from watchdog device
  #   interval: ~
  ## optional close, false disables MAGICCLOSE flag read from device
  ## watchdog will be kicked instead
  #   close: false
  ## by default migration requires some network manager config to be present (eg from wlan or supplied)
  ## set this to false to not require connection files
  require_nwmgr_config: ~
balena:
  image:
  ## use dd / flash balena image
    dd:
      path: balena-cloud-beagleboard-xm-2.38.0+rev1-v9.15.7.img.gz
  #   hash:
  #     md5: <MD5 Hash>
  ## or
  ## use filesystem writes instead of Flasher (dd)
  # fs:
  ## needed for filesystem writes, beagleboard-xm masquerades as beaglebone-black
  #   device_slug: beaglebone-black
  ## make mkfs.ext4 check for bad blocks, either
  ## empty / None, -> No test
  ## Read -> Read test
  ## ReadWrite -> ReadWrite test (slow)
  #   check: Read
  ## maximise resin-data partition, true / false
  ## empty / true -> maximise
  ## false -> do not maximise
  ## Max out data partition if true
  #   max_data: true
  ## use direct io for mkfs.ext (-D see manpage)
  ## true -> use direct io (slow)
  ## empty / false -> do not use
  #   mkfs_direct: ~
  ## extended partition blocks
  #   extended_blocks: 2162688
  ## boot partition blocks & tar file
  #   boot:
  #     blocks: 81920
  #     archive:
  #       path: resin-boot.tgz
  #       hash:
  #         md5: <MD5 Hash>
  ## rootA partition blocks & tar file
      root_a:
        blocks: 638976
        archive:
          path: resin-rootA.tgz
      # rootB partition blocks & tar file
      root_b:
        blocks: 638976
        archive: resin-rootB.tgz
      # state partition blocks & tar file
      state:
        blocks: 40960
        archive: resin-state.tgz
      # data partition blocks & tar file
      data:
        blocks: 2105344
        archive: resin-data.tgz
  # config.json file to inject
  config:
    path: config.json
  #   hash:
  #     md5: <MD5 Hash>

  ## application name
  app_name: 'bbtest'
  ## api checks
  api:
    host: "api.balena-cloud.com"
    port: 443
    check: true
  ## check for vpn connection
  check_vpn: true
  ## timeout for checks
  check_timeout: 20
debug:
  ## don't flash device - terminate stage2 and reboot before flashing
  no_flash: false
```

        
### Stage 2 - balena-stage2 

The initramfs will attempt to start the balena-stage2 executable. 

First steps in stage2 are to determine and mount the configured root partition and read ```/balena-stage2.yml```. 
Before attempting to migrate stage2 will restore the original boot setup to allow the device to reboot into 
its former setup if something goes wrong. To do this other partitions might have be 
remounted. 

The next step is to move all files required by the migrator to initramfs. Typically this is the balena OS image, config.json, 
network manager configurations and the backup.

Once all files are safely copied to initramfs the mounted partitions are unmounted and the balena-os image is 
flashed to the device. Beginning with this process the migration is not recoverable.

If flashing was successful ```balena-stage2```  will attempt to mount the ```resin-boot``` and ```resin-data``` partitions 
and copy config.json, ```system-connections``` files  and the backup. A log of stage2 will also be written to 
```resin-data/migrate.log``` or to the configured log device. 

The device is the rebooted and should start balena-os.   
     


## Windows Migration Strategies

Migrating windows devices to Balena is a challenge, due to the absence of well documented interfaces 
(windows being closed source), the absence of common boot managers like grub. 

As in linux systems ```balena-migrate``` will collect information about the system to determine if it can be migrated 
and to decide on a suitable strategy. 
Currently the only tested strategy works only on EFI enabled systems. ```balena-migrate``` will mount the EFI partition 
and install a migration boot environment using a balena kernel and initramfs that is configured to boot using a 
syslinux EFI boot manager. In this scenario the syslinux boot manager replaces the windows boot manager. The strategy has 
been proven to work, but needs further improvements. Currently unlike on linux there is no fallback strategy 
(boot back to windows) once the migrator has booted into migration initramfs. 




## Next steps

- Detect available space or make space available on the harddisk.
- Try to programatically create a new partition and write a bootable linux image to it.
- Try to use BCDEdit or other available tools/interfaces to make the partition boot.
- Try to set up a minimal linux to do migration after being booted.
