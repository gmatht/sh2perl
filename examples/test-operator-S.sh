#!/bin/bash
# Test operator -S (socket) in single bracket
if [ -S /dev/null ]; then
    echo "is a socket"
fi
# Test operator -S in double bracket
if [[ -S /dev/null ]]; then
    echo "is a socket"
fi
