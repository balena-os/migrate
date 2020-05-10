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
- intel-nuc x86_64 devices, using Ubuntu flavors:
  - Ubuntu 18.04.3 LTS
  - Ubuntu 18.04.2 LTS
  - Ubuntu 16.04.2 LTS
  - Ubuntu 14.04.2 LTS
  - Ubuntu 14.04.5 LTS
  - Ubuntu 14.04.6 LTS
  - Windows 10 professional with UEFI bootmanager 
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
 
You will need root access to migrate a device. 

To migrate a device a minimum of 512MiB memory and 200MiB of free disk space for image download is required. 
For device-types using flasher type images like x86 devices or beaglebones, the additional disk space required for 
image download and extraction is ~2.7GiB. 
 
The additional disk space requirements can be avoided by supplying the balena-OS image instead of downloading. 
Please be aware that when providing the image for intel or beaglebone devices, the actual OS image has to be 
extracted from the flasher image obtainable from the dashboard. This process is explained in section 
```Providing the Balena Image``` below.     
  
## How To

**Warning** - Please be aware that migrating devices across operating systems is a complex task and a lot can go wrong 
in the process. In the worst case the  device can get stuck in migration or fail to boot.  

Before attempting to migrate a device or a fleet, please try your setup on one or more test devices that reflect the 
state of your fleet and are easily accessible for manual reboot or reflash in case something goes wrong.      

### Quickstart 


As of version 0.2.0 the minimum requirements for migration are a version of ```balena-migrate``` compatible to your 
device and a ```config.json```  file downloaded from the dashboard for the application you want the device to be migrated to.

```bash
thomas@balena-u14-2:~/migrate$ ./balena-migrate --help
balena-migrate 0.2.0
Thomas Runte <thomasr@balena.io>
Migrate a device to BalenaOS

USAGE:
    balena-migrate [FLAGS] [OPTIONS]

FLAGS:
    -d, --def-config      Print a default migrate config to stdout
    -h, --help            Prints help information
        --no-flash        Debug mode - do not flash in stage 2
    -n, --no-nwmgr-cfg    Allow migration without network config
        --no-os-check     Do not fail on un-tested OS version
    -p, --pretend         Run in pretend mode - only check requirements, don't migrate
    -v                    Increase the level of verbosity

OPTIONS:
    -c, --config-json <FILE>       Select balena config.json
    -i, --image <FILE>             Select balena OS image
        --migrate-config <FILE>    Select migrator config file
    -r, --reboot <DELAY>           Reboot automatically after DELAY seconds after migrate setup has succeeded
        --version <VERSION>        Select balena OS image version for download
    -w, --work-dir <DIR>           Select working directory
```

All assets other than ```config.json``` are included in ```balena-migrate``` or can be automatically downloaded. To avoid downloads during 
migration the balena OS image can be supplied and configured at the command line using the ```-i``` or ```--image``` option.  

Optionally you can supply a configuration file, that allows you to specify several advanced options. 
The configuration file is in YAML format and can either be provided as ```balena-migrate.yml``` in the current directory
or specified using command line option ```--migrate-config```. ```balena-migrate``` will print a default config file 
to stdout when invoked with the ```-d``` command line flag.

```balena-migrate``` uses a working directory to access and save files. The working directory is where the program 
expects all files you provide to be present. The disk space requirements for image downloads (200MiB for non flasher images, 
2.7GiB for flasher images) apply to the drive the working directory resides on. 
Files expected in the working directory include ```config.json``` and optionally ```balena-migrate.yml```, the balena OS image and possible network manager files you provide. 

By default the working directory will be the current directory. You can specify an alternate directory using the ```-w``` or 
```--work-dir``` command line option.     

The most simple way to start migration is to place your ```config.json``` in a directory with sufficient space and call 
```balena-migrate``` as follows: 

```shell script
sudo ./balena-migrate -c config.json  
``` 

**Automatic Reboot**

To automatically boot ```<delay>``` seconds after setup is complete add the ```-r <delay>``` option: 

```shell script
sudo ./balena-migrate -c config.json -r 5  
``` 

**Selecting the Balena_OS Image**

```balena-migrate``` will check requirements and - if all are met - download the default OS image for the 
platform and set up migration. 
If you would like to supply the Balena-OS image you can do so using the ```-i <image-file>``` option:

```shell script
sudo ./balena-migrate -c config.json -i balena-cloud-intel-nuc-2.48.0+rev3.prod.img.gz   
``` 
The supplied image needs to be gzip-compressed in contrast to zip-compressed (the download format of the balena dashboard).
In case of intel-nuc, Generic X86_64, beagebone or other flasher type devices the actual OS image needs to be extracted 
from the flasher image. This process is explained in **Providing the Balena Image** below. 

To select a specific version of Balena-OS for download you can use the ```--version <version-spec>``` option. 

To select a specific version:
```shell script
sudo balena-migrate -c config.json --version 2.48.0+rev3.prod
```
The ```^``` and ```~``` syntax can be used to select a version greater in the same major or minor range:

```shell script
sudo balena-migrate -c config.json --version ~2.48
```

**Disable Network Configuration Checks**

```balena-migrate``` will automatically scan your wifi configuration and attempt to derive a network setup for 
balena-OS. If no network setup is created, migration will fail with an error. If you are sure that your device will 
be able to come online without network setup (eg. if the device is connected using a wired network on the standard ports) 
you can disable this check using the ```-n```  option.    

#### Providing the Balena Image

```balena-migrate``` needs a balena-OS image file which will be flashed to the device in stage 2 of migration. This file can be 
downloaded automatically by ```balena-migrate``` or provided. 
A balena-OS image manually downloaded from the dashboard can **not** be used with the migrator directly. 
The migrator needs to operate with as little diskspace as possible when flashing the image because the 
image will temporarily be stored in memory while the disk is being flashed. For this reason the migrator uses a gzip 
compressed image that can be streamed directly to dd in contrast to the zip compresssed image provided by the dashboard 
download.
 
For several device-types the image downloaded from the dashboard is a flasher image that contains the 
actual balena-os image. In this case the image needs to be extracted from the flasher image.
Extracting the image requires up to 2.7GiB of disk space. ```balena-migrate``` will extract the balena-OS image 
for you on download, if sufficient space is available.      

The easiest way to provide a valid balena-OS image for subsequent use is to run ```balena-migrate``` in pretend mode (```-p``` option) and
specify the image to download. For flasher type devices make sure you have at least 2.7GiB available disk space.
You still need to provide a config.json so to download the default version of balena-OS, invoke ```balena-migrate``` as follows:
```bash
sudo ./balena-migrate -p -c config.json
```
To download a specific version, use the ```--version``` option:
```bash
sudo ./balena-migrate -p --version ^2.48 -c config.json
```
On success the downloaded/extracted file can be found in the current directory. This file can then be used for migration 
using the ```-i``` option.
 
**Manual preparation of the balena-OS image**

This repository provides a script in ```script/extract.sh``` that will extract all required files from a flasher image and save them in 
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

By default ```balena-migrate``` will scan the device for wifi configurations and attempt to migrate them to 
NetworkManager connection files. There is plenty of room for improvement here - currently scanning network configs 
is very basic (only SSID & secret key) and supports only wifi configurations in wpa_supplicant, conmanager and 
NetworkManager format. The SSID's that are migrated are determined by two flags in ```balena-migrate.yml```. The 
```all_wifis``` flag when when set to true (default) will attempt to migrate all wifi configurations found. The ```wifis``` flag
consists of a list of ssids. Only ssids contained in the list wil be migrated.

If no network configurations are migrated ```balena-migrate``` will refuse to migrate the device, to not create a device 
that can not come online. This behaviour can be overridden by setting the ```-n``` command line flag with 
```balena-migrate``` or setting ```require_nwmgr_config``` to ```false``` in ```balena-migrate.yml```.

Further network configuration can be supplied in NetworkManager connection files and configured using the 
```nwmgr_files```  parameter in ```balena-migrate.yml```.   

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
    fs:
      extended_blocks: 458752
      device_slug: beaglebone-green
      check: ~
      max_data: ~
      mkfs_direct: ~
      boot:
        blocks: 81920
        archive: resin-boot.tgz
      root_a:
        blocks: 638976
        archive: resin-rootA.tgz
      root_b:
        blocks: 638976
        archive: resin-rootB.tgz
      state:
        blocks: 40960
        archive: resin-state.tgz
      data:
        blocks: 401408
        archive: resin-data.tgz
``` 

The above config snippet must be added to ```balena-migrate.yml``` and the partition archives 
(```resin-xxx.tgz```) need to be present in the working directory. 

```check``` can be set to ```ro``` or ```rw``` to perform a read-only or read-write check while formatting 
the device. This is achieved by adding the ```-c``` option to the invocation of mkfs in case of 
read-only or ```-cc```  in case of read-write.

```mkfs_direct``` can be set to true use direct io. This will add the ```-D``` option to the invocation of mkfs.

```max_data``` can be set to true to use all available data for the resin-data partition.  

### Compiling and configuring the Migrator

#### Compiling the Migrator Executables

The tools necessary for cross compiling and compiling for musl must be installed.

Due to the complex setup involved in creating cross compiled and statically linked binaries in rust the project 
currently uses the [rust-embedded cross][https://github.com/rust-embedded/cross] cross compilation tools to compile. 
The rust-embedded/cross project introduces the **cross** command that replaces the regular rust **cargo** command. 
 
Creating a version of ```balena-migrate``` for a platform with assets included is done by as script that can be found 
in ```script/mk_migrator```. The script is called with your normal user but will ask for root 
privileges which are needed to process the initramfs image that will be included in ```balena-migrate```.
Call the script from the base directory of the repository as follows:
For intel-nuc devices: 
```bash
script/mk_migrator -d intel-nuc 
```  

For raspberry PI 3 devices: 
```bash
script/mk_migrator -d raspberrypi3 
```  
