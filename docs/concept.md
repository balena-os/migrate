# Migrate Concept

Migration takes place in two stages that are described in detail in the following chapters.

## Stage 1 

I stage 1 a platform dependent executable is executed with admin privileges. 
Command line options determine the primary mode of operation which can be either

- Standalone - The stage 1 executable will rely on local resources and not try to establish a connection to the balena cloud backend. All resources including the Balena OS image and the migration envoronment have to be supplied locally.

- Agent Mode - The stage 1 executablr acts as an agent. It tries to connect to the Balena cloud backend. The Balena OS image can be dynamically configured and downloaded from the backend. As long term goal the agents integrates with the dashboard and allows the configuration and initiation of the migration  from the dashboard. Functionality currently implemented in migdb (fleet migration) can be partially or completely implemented in the dashboard.

### Configuraton 

The stage 1 executable can be configured using command line parameters and / or a configuration file. 
Configuration file can be in yaml syntax.


### Standalone Migration

Required resources are:

- A migration environment containing:
    - Migration kernel & initramfs
    - for UEFI environments: uefi loader
    - for Legacy systems 
- A balenaOS image
- config.json file 

Optional resources are 
- WIFI credentials
- Additional network manager files
- Several options governing the migration process.

### Checked files
The configuration supplied in ```balena-migrate.yml```  contains several files that are required to boot into 
stage2 or are required in stage2. 

The files needed  for stage2 (balena image(s), config, networkmanager files) are required to be located in the 
working directory. 
```balena-migrate``` will configure the working directory for stage2 either as a path into the root directory or as a 
mount of its own. Restricting the stage2 files location to the working directory ensures that by mounting 
the working directory, the files will be accessible in stage2. 
     
```balena-migrate``` will also calculate the file sizes for boot files (balena kernel, initramfs and dtb if required) 
and for stage2 files. The total size of the boot files needs to fit into the boot directory (selected by bootmanager). 

The stage2 files need to fit into memory (intramfs ramdisk) while the installation device is being written. 
This is checked first in ```balena-migrate``` based on total available memory and then again in stage2 where the amount 
of memory available can be determined more precisely.          

### Stage 2 Boot Setup

Setting boot is handled by the device and boot_manager traits / modules  and the device / boot manager specific 
implementations. ```device.get_device``` will attempt to return a device specific device trait implementation or fail if 
none is found. 

The device implementation will determine and return a suitable boot_manager implementation or fail if none is found.

The basic strategy for stage2 is to place the kernel, initramfs and possible dtb files in a convenient location 
depending on the boot manager used and boot them. 
The kernel command line provided will typically use partuuid or uuid syntax for the root device which has no relevance 
other than that it contains the stage2 config file ```balena-stage2.yml```. 
For most system (other than x86 / grub) this is the critical of stage2 boot as the stage2 config contains 
information needed to restore the former boot configuration and mount all other required partitions / directories. 
Failing to restore the former boot configuration will lead to lost devices if stage2 fails.    
    
#### x86
On x86 systems (currently only intel-nuc is supported) on Linux there are several different boot managers that can 
be utilized. 
If we are running on a Windows system currently only EFI is supported and ```balena-migrate``` will fail if EFI setup 
can not be found or if secure-boot is detected which is currently not supported. 

On x86 Linux currently only grub is supported. 
It can be challenging to determine the active boot manager when more than one boot manager is present/installed in the 
system. If a grub installation is found on an x86 Linux system ```balena-migrate``` will use grub 'hoping' that this 
configuration will work to reboot the system into stage2. If grub is not the active boot manager the setup should have 
no effect. 

##### grub

If a grub boot manager is detected balena-migrate will attempt to add a new boot configuration in /etc/grub/grub.d 
and activate that configuration using ```update-grub``` and ```grub-reboot``` to allow the stage2 boot configuration 
to boot only once. 
This way - if stage2 fails - the system will return to the former configuration on next boot without the need to restore 
the former configuration. 

The grub boot manager implementation will attempt to place the boot (kernel, initramfs) files in ```/boot``` and the 
stage 2 config file ```balena-stage2.yml``` in the root of that mount (typically ```/``` or ```/boot``` if it is 
mounted as a separate partition).        

##### EFI / Windows

EFI boot is currently used only in Windows migration. The EFI boot manager will attempt to create a balena boot directory 
```\EFI\balena-migrate``` in the EFI partition and place the kernel and initramfs files there.
It will attempt to access or create the default EFI boot directory ```\EFI\Boot``` and place a file ```startup.nsh``` there that 
will boot the balena configuration. The last step is to move the default windows boot files (```\EFI\Boot\bootx64.efi```) 
to ```\efi_backup``` to make sure they will not override our configuration. 
It might become necessarry to move more files if other boot configurations are present.  

The EFI boot setup is not complete yet and might have to get modified. Placing the kernel and initramfs in the EFI 
partition allows us to get away without repartitioning and creating a boot partition for stage2, as the kernel is unlikely 
to boot from NTFS partitions otherwise present in Windows installations. 
If the EFI partition is too small or too full this strategy will fail. 

The x86 balena kernels used,  need to be compiled with NTFS support to allow access to the working directory which will 
be placed on the windows boot partition (typically c:\). 

As linux device names are hard to compute from the device information present in windows we require partuuids to setup 
stage2 boot and access to the working directory. ```balena-migrate``` will fail if partuuids can not be retrieved for the 
EFI partition and the partition containing the working directory. 

#### RPi 

The RPi boot manager will attempt to place the stage2 kernel & initramfs in /boot which typically resides in a 
separate partition on the RPi. 
The files ```/boot/config.txt``` and ```/boot/cmdline.txt``` will be backed up to ```/boot/{filename}.TS``` and the 
original files are modified to boot the stage2 configuration. 
The stage2 configuration will also be placed in ```/boot``` and the boot partition will be used as root.   

#### UBoot Platforms  

u-boot boot configurations be setup up in several different ways and the challenge for the balena migrate u-boot boot 
manager is to understand the configuration presnt on the device.
u-boot files (MLO & u-boot.img) that indicate the partition u-boot boots from can be found in regular partitions, 
in the boot sector of a drive or in special mmcblk devices (mmcblkboot). 
The current strategy is to use a uEnv.txt file to modify the boot configuration. The location of the file has 
to carefully chosen to allow u-boot to find it and use it appropriately. 

Possible complications can result from incompatibilities between the u-boot files and the kernel / dtb files.

Current strategy is to scan all available partitions for u-boot files and choose the root of the first partition that contains
these files as location for the uEnv.txt file. The balena stage2 configuration ```balena-stage2.yml``` will be created 
in the same location and the partition will be set up as root partition in the kernel command line. 
