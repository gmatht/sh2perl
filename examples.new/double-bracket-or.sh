#!/usr/bin/env bash
# Demonstrates [[ ... ]] || [[ ... ]] in if condition
if [[ -d /efi/Default ]] || [[ -d /boot/Default ]]; then
    echo "found"
fi
