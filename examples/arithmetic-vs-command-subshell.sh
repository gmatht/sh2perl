#!/bin/sh
# $(( ambiguous - could be arithmetic or command subshell
result=$((echo "test"))
echo done
