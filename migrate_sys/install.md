# Installing the migrate system

**ADMIN privileges are required** - the following has to be executed in an admin shell.

Install required files to EFI partition as follows:
```
# mount EFI partition to free drive letter
mount b: /s
# copy the pre-configured EFI directories
xcopy /s $SOURCE_DIR\migrate_sys\* b:\
# copy kernel and initramfs
copy $KERNEL_DIR\kernel.xz  B:\balena\x86_64\
copy $KERNEL_DIR\rootfs.xz  B:\balena\x86_64\
# copy sytemd_boot efi executable 
copy $SYSTEMD_BOOT\bootx64.efi B:\EFI\BOOT\
copy $SYSTEMD_BOOT\bootx64.efi B:\EFI\Migrate\BOOT\
```

Now try to activate the EFI boot configuration using bcdedit

```
# list all boot entries just for fun
bcdedit /enum all
# make a copy of the windows boot loader
bcdedit /copy {bootmgr} /d "Balena Migrate System"
# will return somethin like:
# > Der Eintrag wurde erfolgreich in {d5f006cd-48a7-11e8-9e1a-bfde62bae14c} kopiert.
# use {d5f006cd-48a7-11e8-9e1a-bfde62bae14c} to reference the created entry
bcdedit /set {d5f006cd-48a7-11e8-9e1a-bfde62bae14c} path \EFI\Migrate\BOOT\BOOTx64.efi
# activate the new entry, better would be to make it start only once
bcdedit /set {fwbootmgr} displayorder {d5f006cd-48a7-11e8-9e1a-bfde62bae14c} /addfirst
# reboot system
```