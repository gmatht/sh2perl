#!/bin/sh
# Case pattern with parentheses - config.sub style
case "$arch" in
    i*86 | x86_64)
        cpu=$basic_machine
        ;;
    *)
        cpu=unknown
        ;;
esac
