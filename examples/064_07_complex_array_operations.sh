#!/bin/bash

# 7. Complex array operations with associative arrays
declare -A config
config["user"]="admin"
config["host"]="localhost"
config["port"]="8080"

# Sort values to avoid hash-order non-determinism between bash and Perl
# NOTE: sort <<<"${config[*]}" receives ONE line (space-joined) and passes
# it through unsorted. Use printf '%s\n' to put each value on its own line
# so sort can work correctly.
IFS=$'\n' sorted=($(printf '%s\n' "${config[@]}" | sort))
echo "Config: ${sorted[@]}"
