#!/bin/bash

# 11. Complex test expressions with extended operators
n="42"
m="debug"
if [[ "$n" =~ ^[0-9]+$ ]] && [[ "$m" == "test" || "$m" == "debug" ]]; then
    echo "Valid input"
fi
