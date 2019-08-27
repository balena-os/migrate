# migrate

Migrate brownfield devices to Balena

This project is based on the ideas from https://github.com/balena-io-playground/balena-migrate and aims to enable 
migration of devices supported by Balena OS from Linux operating systems to Balena OS. 
Work is in progress to allow migration of Windows devices to Balena OS.

The core functionality of the script based project in https://github.com/balena-io-playground/balena-migrate 
has been redesigned and re-implemented in rust to provide a more reliable experience.

## Strategy

### Stage 1 - balena-migrate

Balena migrate consists of a binary executable file that needs to be executed with root privileges on the device 
that will be migrated. There are several command line parameters that can be set and the program will be looking 
for a YAML configuration file - by default in ```./balena-migrate.yml```.

Depending on the configuration ```balena-migrate``` will do one of the following depending on the ```mode``` setting:
- pretend - check requirements for migration but apply no changes to the system. All required settings and files need to 
be present and configured. 
- immediate - check requirements for migration and migrate the system immediately. All required settings and files need 
to be present and configured. 
- extract - extract partitions from image and store their contents as tar files to allow file system 
level writing of balena OS. Will produce a configuration snippet for balena-migrate.yml  

The folowing options are concepts that have been disccussed but are not implemented:
- connected - check requirements for migration and try to retrieve missing files from the balena cloud. 
Migrate immediately once all requirements are met. This mode is not implemented yet. 
- agent - Connect to balena cloud and install ```balena-migrate``` as a service. 
Migration can be configured and triggered from the balena dashboard. This mode is not implemented yet.

In stage 1 ```balena-migrate``` tries to determine the running OS, device architecture and the exact device type. 
Based on that information it decides if the device can be migrated.
For a successful migration ```balena-migrate``` needs to be able to modify the boot setup and boot into a balena kernel 
and initramfs. The files needed are device dependent - usualy a kernel image, an initramfs that contains stage2 executable 
of ```balena-stage2``` and possibly a device tree blob file. These files have to be provided or they an be downloaded 
automatically in ```connected``` or ```agent``` mode. Currently the files are 'custom made' and cannot be downloaded.

```balena-migrate``` also needs a balena OS image file which will be flashed to the device in stage 2 and currently 
requires a config.json file to be provided. These files can be downloaded automatically if a valid application id, and 
api key is provided. The functionality for automatic download is not yet provided in ```balena-migrate```. 

If configured ```balena-migrate``` will scan the device for wifi configurations and attempt to migrate them to 
NetworkManager connection files. ```balena-migrate``` can also be configured to create a backup that will automatically 
be converted to volumes once balena-os is running on the device. 

There is plenty of room for improvement here - currently scanning network configs is very basic and supports only wifi
configurations.       

```balena-migrate``` needs to be able to determine the installation device. Usually it will choose the device that 
contains the boot setup. Unfortunately this task is not trivial as boot partitions could potentially reside on a 
different drive from the root partition which makes it hard to detect and is generally not supported by balena OS. When 
migrating devices with more complex disk layouts and more than one OS ```balena-migrate``` should be used with great 
caution and might not be the right tool at all.        

Once all required files are found balena-migrate will set up the device to boot into the balena kernel and initramfs, 
write a configuration file for stage 2 ```balena-stage2.yml``` and reboot the device.
The kernel is booted using a root device that contains ```balena-stage2.yml``` in the file system root. This will typically be 
the ```/boot``` partition if it is located in a separate partition or any other partition that contains the boot files 
(eg. MLO, uboot.img files for u-boot). If none of the above is available the new root will be the old root partition. 
The root partition will generally be addressed using its partuuid. 
The ```balena-stage2.yml``` will contain al necessary information to restore the former boot configuration and to mount 
and access the working directory, that contains all other required data. 
   
#### Backup Configuration
```balena-migrate.yml``` contains a section for backup configuration. The backups is grouped into volumes - volume names 
corresponding to the top level directories of the backup archive. 
Each volume can be configured to contain a complex directory structure. Volumes correspond with application container 
volumes of the application that is loaded on the device once balena OS is running. 
The balena-supervisor will scan the created backup for volumes declared in the application containers and automatically 
restore the backed up data to the appropriate container volumes. 
The supervisor will delete the backup once this process is terminated. Backup directories with no corresponding volumes 
are not retained. 

*Backup configuration example:*

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
    
### Stage 2 - balena-stage2 

The initramfs will attempt to start the balena-stage2 executable. 

First steps in stage2 are to determine and mount the configured root partition and read ```/balena-stage2.yml```. 
Before attempting to migrate stage2 will restore the original boot setup to allow the device to reboot into 
its former setup if something goes wrong. To do this other partitions might have be 
remounted. 

The next step is to move all files required to initramfs. Typically this is the balena OS image, config.json, 
network manager configurations and the backup.

Once all files are safely copied to initramfs the mounted partitions are unmounted and the balena-os image is 
flashed to the device. Beginning with this process the migration is not recoverable.

If flashing was successful ```balena-stage2```  will attempt to mount the ```resin-boot``` and ```resin-data``` partitions 
and copy config.json, ```system-connections``` files  and the backup. A log of stage2 will also be written to 
```resin-data/migrate.log``` or to the configured log device. 

The device is the rebooted and should start balena-os.   
     

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
- Beaglebone Green / Black and Beagleboard XM using Debian 9 or Ubuntu flavors:
  - Ubuntu 18.04.2 LTS
  - Ubuntu 14.04.1 LTS
  - Debian GNU/Linux 9 (stretch)
    
Further device-types and operating systems will be added as required. Adding a new OS 
is usually trivial, adding a new device might reqire more effort.     

## Example - Setting up Migration in IMMEDIATE mode 

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
    # drive: '/dev/sda1'
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
  # path: balena.dtb
  # hash:
  #   md5: <MD5 Hash>
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



## Windows Migration Strategies

Migrating windows devices to Balena is a challenge, due to the absence of well documented interfaces 
(windows being closed source), the absence of common boot managers like grub. 

As in linux systems ```balena-migrate``` will collect information about the system to determine if it can be migrated 
and to decide on a suitable strategy. 
Currently the only tested strategy works only on EFI enabled systems. ```balena-migrate``` will mount the EFI partition 
and install a migration boot environment using a balena kernel and initramfs that is configured to boot using a
```startup.nsh``` file that is placed in ```\EFI\BOOT```. For this to work the windows EFI boot configuration needs to 
be removed. ```balena-migrate``` will move the windows EFI boot files to a backup directory on the EFI drive. 




## Next steps

- Detect available space or make space available on the harddisk.
- Try to programatically create a new partition and write a bootable linux image to it.
- Try to use BCDEdit or other available tools/interfaces to make the partition boot.
- Try to set up a minimal linux to do migration after being booted.

 