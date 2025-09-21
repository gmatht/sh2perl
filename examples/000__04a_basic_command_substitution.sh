#!/bin/bash

# Basic command substitution examples using backticks
# This file demonstrates simple command substitution using backticks (`)

echo "=== Basic Command Substitution ==="

# Simple command substitution
echo "Current date: `date +%Y`"
#echo "Current user: `whoami`"
echo "Current directory: `pwd | sed 's|.*/||'`"

# Assigning backtick results to variables
current_date=`date +%Y%m`
#current_user=`whoami`
current_dir=`pwd | sed 's|.*/||'`

echo "Stored date: $current_date"
#echo "Stored user: $current_user"
echo "Stored directory: $current_dir"

echo "=== Basic Command Substitution Complete ==="

