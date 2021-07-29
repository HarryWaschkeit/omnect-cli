#!/bin/bash

# include shared functions
. /ics-dm-sh/functions

# exit handler which makes sure we dont leave an undefined host state regarding loop devices
function finish {
    set +o errexit
    umount /tmp/mount/data
    umount /tmp/mount/etc
    umount /tmp/mount/rootA
    losetup -d ${loopdev}
}
trap finish EXIT

function usage() {
    echo "Usage: $0  -c identity_config -w wic_image" 1>&2; exit 1;
}

set -o errexit   # abort on nonzero exitstatus
set -o pipefail  # don't hide errors within pipes

while getopts "c:w:" opt; do
    case "${opt}" in
        c)
            c=${OPTARG}
            ;;
        w)
            w=${OPTARG}
            ;;
        *)
            usage
            ;;
    esac
done
shift $((OPTIND-1))

if [ -z "${c}" ] || [ -z "${w}" ]; then
    usage
fi

d_echo "c = ${c}"
d_echo "w = ${w}"

[[ ! -f ${w} ]] && echo "error: input device image not found" 1>&2 && exit 1
[[ ! -f ${c} ]] && echo "error: input file \"${c}\" not found" 1>&2 && exit 1

# set up loop device to be able to mount image.wic
losetup_image_wic

# search and mount "etc" partion
part_pattern="etc"
mount_part

# search and mount "data" partion
part_pattern="data"
mount_part

# search and mount "rootA" partion
part_pattern="rootA"
mount_part

# copy identity config
aziot_gid=$(cat /tmp/mount/rootA/etc/group | grep aziot: | awk 'BEGIN { FS = ":" } ; { print $3 }')
mkdir -p /tmp/mount/etc/upper/aziot/
d_echo cp ${c} /tmp/mount/etc/upper/aziot/config.toml
cp ${c} /tmp/mount/etc/upper/aziot/config.toml
chgrp ${aziot_gid} /tmp/mount/etc/upper/aziot/config.toml
chmod a+r,g+w /tmp/mount/etc/upper/aziot/config.toml

# activate identity config on first boot depending on device variant (edge / non edge)
# here it is okay to alter a file in the root partition
if [ -e /tmp/mount/rootA/usr/bin/iotedge ]; then
    echo "iotedge config apply" >> /tmp/mount/rootA/usr/bin/ics_dm_first_boot.sh
elif [ -e /tmp/mount/rootA/usr/bin/aziotctl ]; then
    echo "aziotctl config apply" >> /tmp/mount/rootA/usr/bin/ics_dm_first_boot.sh
else
    echo "no binary found to apply config.toml" 1>&2; exit 1;
fi

# config hostname
config_hostname ${c}
