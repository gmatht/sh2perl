#!/bin/sh
# Demonstrates test expression with \( \) -a -o grouping
if [ \( ! -f /tmp/test \) -a \( -d /tmp \) ]; then
    echo "ok"
fi
