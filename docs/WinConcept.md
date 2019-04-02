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

On these systems information about the OS, Boot process and drives can easiy be gathered using the Windows Management Interface (WMI). This interface can also be used to manipulate the system, to partition and format disks and to set up the boot configuration.

So far I have rust interfaces to query information from WMI. Calling  WMI object methods is yet to be implemented.

With a system set up for EFI boot it should be trivial to set up an alternative configuration on the EFI partition. Main challenge is to use BCDEdit or its WMI interfaces to set up the NVRAM variables to boot the new configration. 

As a fallback there is an option to replace the Windows boot setup, so that no NVRAM variable changes are needed. Downside to this approach that it destroys the Windows boot setup. As a result it should only be used as a last resort when NVRAM setup using BCDEdit fails.  

The EFI partitions will usually be several hundred MB in size and thus have sufficient free space to deposit another EFI boot configuration in most cases. In windows the EFI file system can be mounted the mountvol command and most probably also using WMI. It can then be accessed with admin privileges.

In the simplest variant the boot configuration could be a linux kernel that (starting with kernel 3.3 and compiled with EFI Stub support) can act as its own boot loader. Downside to this approach is that no Kernel boot parameters can be specified on the command line - instead they would have to be compiled into the kernel. This makes this approach a little inflexible. 

To get around that we would need to install an intermediate boot manager like syslinux that allows us to configure kernel boot parameters. 

Kernel boot parameters would typically include the path to an initramfs and possibly to a root file system. 

Still the advantage of a UEFI boot configuration remains, in that we have a good chance of getting around having to make space for a boot partition. With a small kernel file and initramfs we can boot a system into ram without needing to write anything outside of the EFI partition at all. The actual OS image could then be downloaded to the ram disk and flashed to hard disk from there.
 
To specify the above parameters we will need to know the device names of the partitions that the files (initramfs, root file system) are stored on and this will is in some cases be a chalenge. 
The linux device names will have to be guessed based on the hard drive information available in windows to produces linux style device names like /dev/sda /dev/mmcblk or /dev/nvme0n1.

The perfect solution would be a self-contained kernel, that contians all necesarry files as well as the root file system (to mount to RAM). 





## Strategies for pre Windows 8

Windows 7 and before will usually be Legacy Bios installations and will be missing 
certain features. I am currently gathering most of the System-Information using WMI. I noticed that powershell is missing some important commands on windows 7 but I have not yet checked if the WMI classes I use are present in these Systems.

The default mechanism for manipulating boot configuration entries in Windows (BCDEdit) appears to work quite differently in Legacy Systems and certain strategies that I am contemplating UEFI migration will not work in Legacy Mode.


 
