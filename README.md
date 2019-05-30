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
- PRETEND - check requirements for migration but apply no changes to the system. All required settings and files need to 
be present and configured. 
- IMMEDIATE - check requirements for migration and migrate the system immediately. All required settings and files need 
to be present and configured. 
- CONNECTED - check requirements for migration and try to retrieve missing files from the balena cloud. 
Migrate immediately once all requirements are met. This mode is not implemented yet. 
- AGENT - Connect to balena cloud and install ```balena-migrate``` as a service. 
Migration can be configured and triggered from the balena dashboard. This mode is not implemented yet.

In stage 1 ```balena-migrate``` tries to determine the running OS, device architecture and the exact device type. 
Based on that information it decides if the device can be migrated.
For a successful migration ```balena-migrate``` needs to be able to modify the boot setup and boot into a balena kernel 
and initramfs. The files needed are device dependent - usualy a kernel image, an initramfs that contains stage2 
of ```balena-migrate``` and possibly a device tree blob file. These files have to be provided or they an be downloaded 
automatically in ```CONNECTED``` or ```AGENT``` mode. Currently the files are 'custom made' and cannot be downloaded.

```balena-migrate``` also needs a balena OS image file which will be flashed to the device in stage 2 and currently 
requires a config.json file to be provided. These files can be downloaded automatically if a valid application id, and 
api key is provided. The functionality for automatic download is not yet provided in ```balena-migrate```. 

If configured ```balena-migrate``` will scan the device for wifi configurations and attempt to migrate them to 
NetworkManager connection files. ```balena-migrate``` can also be configured to create a backup that will automatically 
be converted to volumes once balena-os is running on the device. 

There is plenty of room for improvement here - currently scanning network configs is very basic and supports only wifi
configurations.       

Once all required files are found balena-migrate will set up the device to boot into the balena kernel and imageramfs, 
write a configuration file for stage 2 in /etc/balena-stage2.yml and reboot the device.

### Stage 2 - balena-stage2 

The initramfs will attempt to start the balena-stage2 executable. 

First steps in stage2 are to determine and mount the former root partition and read ```/etc/balena-stage2.yml```. 
Before attempting to migrate stage2 will restore the original boot setup to allow the device to reboot into 
its former setup if something goes wrong. To do this other partitions (/boot the boot manager partition) might be 
re mounted. 

The next step is to move all files required to initramfs. Typically this is the balena OS image, config.json, 
network manager configurations and the backup.

Once all files are safely copied to initramfs the mounted partitions are unmounted and the balena-os image is 
flashed to the device. Beginning with this process the migration is not recoverable.

If flashing was successful ```balena-stage2```  will attempt to mount the ```resin-boot``` and ```resin-data``` partitions 
and copy config.json, ```system-connections``` files  and the backup. A log of stage2 will also be dumped 
in ```resin-data/migrate.log```. 

The device is the rebooted and should start balena-os. 
  
     

## Requirements

Balena migrate currently works on a small set of devices and linux flavors. Tested and working devices are:
- x86_64 devices, tested mainly on VirtualBox using Ubuntu 14 / 16 / 18
- Raspberry PI 3 using up to date Raspian
- Beaglebone Green and Beagleboard XM using Debian 9 and Ubuntu 14 / 18
    
Further device-types and operating systems will be added along the way.    


## Example - Setting up Migration in IMMEDIATE mode 

A (working) sample configuration file:

```yaml
migrate:
  ## select the migrate mode, currently PRETEND | IMMEDIATE
  mode: IMMEDIATE
  ## The working directory, files are expected / placed here
  work_dir: '.'
  ## migrate all wifis found in wpa_supplicant conn_mgr or NetworkManager files
  # all_wifis: true
  ## or migrate selected wifis 
  wifis:
   - QIFI
  ## reboot automatically after given amount of seconds  
  reboot: 5 
  ## log settings
  log:
    ## log level for stage 2 one of error, warn, info, debug, trace
    level: 'debug'
    ## log to an external drive (can not be the drive that will be migrated)
    ## if not set the log will be written to /resin-data or workdir   
    drive: "/dev/sdb1"
  ## The kernel to boot into for stage2 (a balena kernel, please)
  kernel_path: "balena.zImage"
  ## The initramfs to boot into for stage 2
  initrd_path: "balena.initramfs.cpio.gz"
  ## The device tree blob to use to boot into stage 2 (u-boot)
  # dtb_path: "balena.dtb"
  ## currently not supported
  # force_slug: 'bad_ass_device'
  ## backup settings
  backup: 
    ## a volume to create in balena
    - volume: log-files
      ## items inside the volume
      items:
      - source: '/var/log'
        # target: the_logs
        # filter: .*\.log 
  ## require network manager configuration to be present (default true)      
  require_nwmgr_config: false
  ## A List of NetworkManager fles to copy to /system-connections  
  nwmgr_files: 
  #   - 'a file'
  ## Fail mode, how to fail in stage2 One of "Reboot" | "RescueShell"
  fail_mode: Reboot
  ## use rust internal gzip instead of command
  gzip_internal: true
## Balena configuration
balena:
  ## The image file to use. Currently needs to be gzipped image
  image: 'balena-cloud-support-multi-intel-nuc-2.31.2+rev1-dev-v9.11.1.img.gz'
  ## The config.json to use
  config: 'config.json'
  ## app name, not currently used / checked
  app_name: 'test' 
  ## specify alt api settings for AGENT mode, not currently used
  api: 
    host: "api.balena-cloud.com"
    port: 443
    check: true
  ## check vpn connectivity
  check_vpn: true
  ## vpn connectivity check timeout
  check_timeout: 20
## Debug settings  
debug:
  ## skip root check  
  fake_admin: false
  ## boot into stage2 but stop and reboot before flashing  
  no_flash: false
  ## The following is strictly debug!
  ## Don't flash to /root device, use this one instead
  # force_flash_device: /dev/sdb
  ## Don't flash but keep on going as if device was flashed, only makes sense 
  ## in combination with 'force_flash_device'  
  # skip_flash: false
  
```



## Windows Migration Strategies

Migrating windows devices to Balena is a challenge, due to the absense of well documented interfaces (windows being closed source), the absense of common boot managers like grub and mechanisms like initramfs. The Linux migrator uses these mechanisms to manipulate / overwrite the root file system and install new a new OS.

On windows so far there are only general ideas on how to boot install a different OS, which are currently being evaluated.
The strategy that is currently being investigated is:

Boot into a minimal Linux

- Find / create space on a bootable harddisk (e.g. 9MB required for minimal linux) 
- Create a partition and write a minimal linux to that partition.
- Configure Windows Boot manager to boot that system. 
- Use the minimal linux to migrate to Balena.

The existing source interfaces with the windows API, WMI and Powershell and other tools to gather information about the installed system. This part is working and supplies the following information:

### Operating System details

To decide wether it is possible to migrate a device we need detailed information about OS version and details about the boot process:
- OS version and release
- boot mechanism EFI / Legacy
- hardware platform
- available memory
- boot device
- ensure we are being executed with admin rights
- make sure that the system is not using secure boot

This information is gathered using WMI and Powershell.

Sample Output:

```
OS Name:          Microsoft Windows 10 Home
OS Release:       10.0.17134
OS Architecture:  AMD64
UEFI Boot:        true
Boot Device:      "\\Device\\HarddiskVolume2"
PhysicalMemory:   16686048
Available Memory: 10822080
Is Admin:         true
Is Secure Boot:   false
```

### Hard Disk Details

Second we need detailed information about the harddisk layout to detect avaialble space. Windows gives us the possibilty to resize life partitions so min / max volume sizes are gathered too. 

This information is gathered using WMI and Powershell.

Sample output: 

```
type: PhysicalDrive
  harddisk index:     0
  device:             \Device\Harddisk0\DR0
  wmi name:           VBOX HARDDISK
  media type:         Fixed hard disk media
  bytes per sector:   512
  partitions:         2
  compression_method:
  size:               39 GiB
  status:             OK

    type: HarddiskPartition
    harddisk index:   0
    partition index:  0
    device :          \Device\HarddiskVolume1
    boot device:      true
    bootable:         true
    type:             GPT: System
    number of blocks: 1024000
    start offset:     1048576
    size:             500 MiB

    type: HarddiskPartition
    harddisk index:   0
    partition index:  1
    device :          \Device\HarddiskVolume2
    boot device:      false
    bootable:         false
    type:             GPT: Basic Data
    number of blocks: 82595840
    start offset:     659554304
    size:             39 GiB
    logical drive:    C:
    min supp. size:   26 GiB
    max supp. size:   39 GiB
```

## Next steps

- Detect available space or make space available on the harddisk.
- Try to programatically create a new partition and write a bootable linux image to it.
- Try to use BCDEdit or other available tools/interfaces to make the partition boot.
- Try to set up a minimal linux to do migration after being booted.

 