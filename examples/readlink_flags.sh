#!/bin/bash
# Test various readlink flags
target_e=$(readlink -e "$1")
target_m=$(readlink -m "$1")
target_f=$(readlink -f "$1")
echo "Canonical (existing): $target_e"
echo "Canonical (missing):  $target_m"
echo "Canonical (full):     $target_f"
