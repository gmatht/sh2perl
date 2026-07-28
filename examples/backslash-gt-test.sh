#!/bin/bash
# Escaped > in test expression (string comparison)
a=5
b=10
if [ "$a" \> "$b" ]; then
    echo "a > b"
fi
