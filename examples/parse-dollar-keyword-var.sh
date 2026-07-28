#!/bin/bash
exec="/usr/sbin/dkms"
prog=${exec##*/}
test -f $exec || exit 0
