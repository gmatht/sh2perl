#!/bin/bash
target=$(readlink -f "$1")
echo "Canonical path: $target"
