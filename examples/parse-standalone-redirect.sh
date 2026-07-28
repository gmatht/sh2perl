#!/bin/sh
# Test: standalone redirect (truncate file, no command) should parse.
>somefile
echo "done"
rm somefile
