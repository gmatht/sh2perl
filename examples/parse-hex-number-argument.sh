#!/bin/bash
# Test: sh2perl can parse hex number tokens (0x...) as command arguments.
echo 0x1234
hex_val=$((0xFF))
echo "hex_val: $hex_val"
echo "done: $?"
