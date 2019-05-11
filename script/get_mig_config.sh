#!/bin/bash

SCRIPT_NAME=$(basename "${0}")

##########################################
# log functions
##########################################

function color {
  if [ -n "$1" ] ; then
    YELLOW='\033[1;33m'
    BROWN='\033[0;33m'
    GREEN='\033[0;32m'
    RED='\033[0;31m'
    NC='\033[0m' # No Color
  else
    YELLOW=
    BROWN=
    GREEN=
    RED=
    NC=
  fi
}

function inform {
    local ts
    ts=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    printf "${GREEN}[%s %s] INFO:${NC} %s\n" "$ts" "${SCRIPT_NAME}" "${1}"
}

function warn {
    local ts
    ts=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    printf "${YELLOW}[%s %s] WARN:${NC} %s\n" "$ts" "${SCRIPT_NAME}" "${1}"
}

function debug {
    if [ "$LOG_DEBUG" == "TRUE" ] && listContains "$DEBUG_FUNCTS" "${1}" ; then
      local ts
      ts=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
      printf "${BROWN}[%s %s] DEBUG:${NC} %s: %s\n" "$ts" "${SCRIPT_NAME}" "${1}" "${2}"
    fi
}

function simulate {
    local ts
    ts=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    printf "${GREEN}[%s %s] INFO:${NC}  %s\n" "$ts" "${SCRIPT_NAME}" "${1}"
}

function clean {
    return
}

##########################################
# fail : try to resotore & reboot
##########################################

function fail {
    local ts
    ts=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    printf "${RED}[%s %s] ERROR:${NC} %s\n" "$ts" "${SCRIPT_NAME}" "${1}"
    clean
    exit -1
}

##########################################
# Evaluate command line arguments
##########################################

function printHelp {
cat << EOI
  get_mig_env
    Get migration env from build host
    USAGE get_mig_env [OPTIONS]
    please run as root
    OPTIONS:
      -d | --device <device type>   - Use the specified device type, defaults to 'raspberrypi'
      -e | --extract                - Extract the contents of initramfs
      -h | --help                   - Print this help and exit
      --host <hostname>             - Use the specified build host name, defaults to 'balena-nuc'
      -m | --model <device model>   - Use the specified device model, defaults to '3'
      -t | --transfer               - Transfer files from build host
      --target <directory>          - Use the specified target directory, defaults to './'
      -u | --user <username>        - Use the specified user name to connect, defaults to 'thomas'
EOI
  exit 0
}

function getCmdArgs {
  # Parse arguments
  while [[ $# -gt 0 ]]; do
    arg="$1"
    case $arg in
      -h| --help)
        printHelp
        exit 0
        ;;
      --host)
        if [ -z "$2" ]; then
           fail "\"$1\" argument needs a value."
        fi
        MAKE_HOST_NAME="$2"
        shift
        ;;
      -e | --extract)
        DO_EXTRACT="Y"
        ;;
      -u|--user)
        if [ -z "$2" ]; then
           fail "\"$1\" argument needs a value."
        fi
        MAKE_HOST_USER="$2"
        shift
        ;;
      -d|--device)
        if [ -z "$2" ]; then
           fail "\"$1\" argument needs a value."
        fi
        DEV_TYPE="$2"
        shift
        ;;
      -m|--model)
        if [ -z "$2" ]; then
           fail "\"$1\" argument needs a value."
        fi
        DEV_MODEL="$2"
        shift
        ;;
      -t | --transfer)
        DO_TRANSFER="Y"
        ;;
      --target)
        if [ -z "$2" ]; then
           fail "\"$1\" argument needs a value."
        fi
        TARGET_DIR="$2"
        shift
        ;;

      *)
        fail "invalid command $arg"
        ;;
    esac
    shift
  done
}

DO_TRANSFER=
DO_EXTRACT=
MAKE_HOST_NAME="balena-nuc"
MAKE_HOST_USER="thomas"
MAKE_HOST_PASSWD=
MAKE_HOST_PATH="/media/thomas/003bd8b2-bc1d-4fc0-a08b-a72427945ff5/balena.io/balena-os"
DEV_TYPE="raspberrypi"
DEV_MODEL="3"
TARGET_DIR="."


color "ON"

getCmdArgs "$@"

if [ -n "$DO_TRANSFER" ]; then
    if [ "$DEV_TYPE" == "raspberrypi" ]; then
        if  [ "$DEV_MODEL" == "3" ]; then
            DEV_SLUG="raspberrypi3"
        else
            fail "unknown device model ${DEV_MODEL} for device type ${DEV_TYPE}"
        fi
    elif [ "$DEV_TYPE" == "beaglebone" ]; then
        if  [ "$DEV_MODEL" == "green" ]; then
            DEV_SLUG="${DEV_TYPE}-${DEV_MODEL}"
        else
            fail "unknown device model ${DEV_MODEL} for device type ${DEV_TYPE}"
        fi
    else
        fail "unknown device type ${DEV_TYPE}"
    fi

    if [ -n "$MAKE_HOST_PASSWD" ]; then
        SCP_CMD=
    else
        SCP_CMD="scp ${MAKE_HOST_USER}@${MAKE_HOST_NAME}:${MAKE_HOST_PATH}/balena-${DEV_TYPE}/build/tmp/deploy/images/${DEV_SLUG}"
    fi

    CURR_CMD="${SCP_CMD}/zImage-initramfs-${DEV_SLUG}.bin ${TARGET_DIR}/balena.zImage"
    inform "attempting ${CURR_CMD}"
    $CURR_CMD || error "failed at command ${CURR_CMD}"
    CURR_CMD="${SCP_CMD}/resin-image-initramfs-${DEV_SLUG}.cpio.gz ${TARGET_DIR}/balena.initramfs.cpio.orig.gz"
    $CURR_CMD || error "failed at command ${CURR_CMD}"

    inform "success!"
fi

if [ -n "$DO_EXTRACT" ]; then
    mkdir -p "${TARGET_DIR}/extract"
    LAST_PWD=$(pwd)
    cd "${TARGET_DIR}/extract"
    gzip -c -d "${TARGET_DIR}/balena.initramfs.cpio.orig.gz" | sudo cpio -i || fail "failed to unpack initramfs to ${TARGET_DIR}/extract"
    cd $LAST_PWD
fi








