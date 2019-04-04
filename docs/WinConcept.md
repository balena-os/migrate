# Brownfield Migration of Windows Devices - intermediate 

Looking at options to migrate Windows devices to Balena I have come up with the following findings.

Depending on the type of Windows installation there are several different cases to look at that might require different strategies. We should probably try to prioritize certain common / likely configurations:

* Windows Version - mainly pre Windows 8 and after. Pre windows 8 will typically be legacy boot and will be missing several features.

* Type of boot process: Legacy or EFI

* Secure Boot

## Problems

### Secure Boot

The main show stopper I currently see is an enabled secure boot option. 
In this case all boot loaders and kernels we want to install on the system would need to be signed. Secure boot can typically only be disabled manually in the BIOS settings.

### Network Configuration

So far I have not dug into Windows Network Configuration. Currently I do not assume that retrieving Wifi credentials will be as easy as in linux. Wifi passwords will likely have to be provided by the user to be able to log on to networks.

### Network Connectivity during Migration

Definitely a 'Nice to Have' feature would be having network connectivity during migration. Unfortunately this would require the migration ENV to provide all the neccessary drivers and firmware for Wifi adapters. 


## Strategies for Post Windows 8 and UEFI Installations

The investigation so far has been restricted to post Windows 8 and EFI installations.

On these systems information about the OS, Boot process and drives can easily be gathered using the Windows Management Interface (WMI). This interface can also be used to manipulate the system, to partition and format disks and to set up the boot configuration.

So far I have rust interfaces to query information from WMI. Calling  WMI object methods is yet to be implemented.

With a system set up for EFI boot it is trivial to set up an alternative configuration on the EFI partition. 

I am currently installing systemd-boot as EFI boot manager. Systemd-boot automatically recognizes the windows boot setup and creates a boot menu entry for it. Configuring systemd-boot to boot a linux is easily done by wrintig two config files in the EFI partition. The kernel and initramfs can reside directly in the EFI partition. Only requirement for the kernel is that it is compiled with EFI_STUB support so it can be started directly by systemd-boot. 

When testing on VirtualBox it is essential to setup the systemd-boot efi executable to repplace the windows vesion in /EFI/BOOT as VirtualBox currently does not persist NVRAM variables across power downs.

Main challenge is to use BCDEdit or its WMI interfaces to set up the NVRAM variables to boot the new configuration. When replacing the Windows default EFI boot loader in /EFI/BOOT/bootx86.efi this is not necessarry.

So far I have not found options to set up single boot using BCDEdit or systemd-boot. This makes recovering from a failing boot problematic. Onece starting the migrate-system can reset the boot manager but otherwise manual intervention is needed. 

The EFI partitions will usually be several hundred MB in size and thus have sufficient free space to deposit another EFI boot configuration in most cases. In windows the EFI file system can be mounted the mountvol command and likely also using WMI. It can then be accessed with admin privileges.

In the simplest variant the boot configuration could be a linux kernel that (starting with kernel 3.3 and compiled with EFI Stub support) can act as its own boot loader. So far I have not been successfull. This setup (without boot manager) does not allow to specify kernel cmdline parameters which are needed to point to the initramfs. Statically compiling the CMDLINE into the kernel difd not work for me and would be a little inflexibale anyway.

Next challenge is to get access to a BalenaOS Image to flash to the device. Currently I am exploring the option of using a kernel with built in NTFS support. The anticipated outcome would be that I can place the BalenaImage in a well known location in Windows and just access it directly from the minimal linux. This would allow me to just use available space on the windows partition and get around having to make space and format a partition to store data. 




## Strategies for pre Windows 8

Windows 7 and before will usually be Legacy Bios installations and will be missing 
certain features (yet to be explored). I am currently gathering most of the System-Information using WMI. I noticed that powershell is missing some important commands on windows 7 but I have not yet checked if the WMI classes I use are present in these Systems.

The default mechanism for manipulating boot configuration entries in Windows (BCDEdit) appears to work quite differently in Legacy Systems and certain strategies that I am contemplating in UEFI migration will not work in Legacy Mode.

In contrast to the stragegy described above I will need to create a partition (most likely FAT32) to contain at least the migrate system (kernel and initramfs, currently 18MB). Once booted from there I can proceed as in EFI configurations.

## Migration scipts

Currently bash / sh scripts are used for linux migration. These scripts can be adapted to be used in the  

 
