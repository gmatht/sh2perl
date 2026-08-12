#!/bin/sh
# Minimal test: mv command with unknown option (-Z) triggers shell fallback.
# The generator used to produce qx{$mv_cmd}, now produces system $mv_cmd_str.
touch src.txt dest.txt
mv -Z src.txt dest.txt 2>/dev/null
echo "mv rc: $?"
rm -f src.txt dest.txt
