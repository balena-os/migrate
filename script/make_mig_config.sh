#!/bin/bash

$DEVICE_TYPE=$1
$DEVICE_MODEL=$2

# scp balena-nuc:/media/thomas/003bd8b2-bc1d-4fc0-a08b-a72427945ff5/balena.io/balena-os/balena-beaglebone/build/tmp/deploy/images/beaglebone-green/zImage-initramfs-4.14.53+gitAUTOINC+f77e7b554e-r22b-beaglebone-green-20190508104806.bin balena.zImage
# scp balena-nuc:/media/thomas/003bd8b2-bc1d-4fc0-a08b-a72427945ff5/balena.io/balena-os/balena-beaglebone/build/tmp/deploy/images/beaglebone-green/resin-image-initramfs-beaglebone-green-20190508232813.rootfs.cpio.gz balena.initramfs.cpio.orig.gz

$REMOTE_BASE_PATH="003bd8b2-bc1d-4fc0-a08b-a72427945ff5/balena.io/balena-os"
$REMOTE_PATH="balena-nuc:${REMOTE_BASE_PATH}/balena-${DEVICE_TYPE}/build/tmp/deploy/images/${DEVICE_TYPE}-${DEVICE_MODEL}"
$REMOTE_KERNEL="${DEVICE_TYPE}-${DEVICE_MODEL}"

# cd extract
# gzip -c -d ../balena.initramfs.cpio.orig.gz | sudo cpio -i

#sudo rm init.d/8*
#sudo rm init.d/9*
#cp "${PROJECT_ROOT}/script/82_migrate" init.d
# cp ${PROJECT_ROOT}/target/${ARCH}/release/balena-stage2 bin

# cp ../../../target/armv7-unknown-linux-gnueabihf/release/balena-stage2 bin
# find . | cpio --quiet -o -H newc | gzip -c > ../balena.initramfs.cpio.gz








