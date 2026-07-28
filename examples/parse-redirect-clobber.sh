#!/bin/sh
# Test: >| (RedirectOutClobber) syntax
# Force truncate output file, ignoring set -C
set -C
: >| /tmp/testfile.txt
echo "done"
