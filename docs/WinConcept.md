# Brownfield Migration of Windows Devices - intermediate 

Looking at options to migrate Windows devices to Balena I have come up with the following findings.

Depending on the Windows installation there are several different cases to look at:

a) Windows Version - mainly pre Windows 7 and before and Windows 8 and after

b) Type of boot process: Legacy or EFI

c) Secure Boot

## Problems

The main show stopper I currently see, is an enabled secure boot option. 
In this case all boot loaders and kernels we want to install on the system would need to be signed. Secure boot can typically only be disabled manually in the BIOS settings.

## Strategies for Post Windows 8 and UEFI Installations

So far I have mainly looked at post Windows 8 and EFI installations. 
On these systems information about the OS, Boot process and drives can easiy be gathered using the Windows Management Interface (WMI). This interface can also be used to manipulate the System, partitioning and formating disks and to set up the boot configuration. 
So far I have rust interfaces to query information from WMI. Calling  WMI object methods is yet to be implemented. 

With a system set up for EFI boot it should not be too hard to set up an alternative configuration on the EFI partition. 
The EFI partitions will usually be several hundred MB in size and have sufficient free space to deposit another EFI boot configuration. It can be mounted using the mountvol command and most probably also using WMI.

In the simplest variant the boot configuration could be a linux kernel that starting with kernel 3.3 and compiled with EFI Stub support can act as its own boot loader. Downside to this approach is that no Kernel boot parameters can be specified on the command line - instead they would have to be compiled into the kernel. This makes this approach a little inflexible. Kernel boot parameters would typically include the path to an initramfs and possibly a root file system. 
Generally this is one 
 
To specify the above parameters we will need to know the device names of the partitions the files are stored on and this is generally a problem, as the linux device names might be hard to guess from within windows. 
   
Windows 7 and before will usually be Legacy Bios installations and will be missing certain features. I am currently gathering most of the System-Information using WMI. I noticed that powershell is missing some important commands on windows 7 but I have not yet checked if the WMI classes I use are present in these Systems.

The default mechanism for manipulating boot configuration entries in Windows (BCDEdit) appears to work quite differently in Legacy Systems and certain strategies that I am contemplating UEFI migration will not work in Legacy Mode.


 
