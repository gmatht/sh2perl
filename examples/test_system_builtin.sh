#!/bin/bash

# This script should generate system calls with builtin commands
echo "Testing system calls with builtin commands"

# These should generate system 'ls' and system 'find' calls
result1=`ls -la`
result2=`find . -name "*.txt"`

echo "Results:"
echo "$result1"
echo "$result2"
