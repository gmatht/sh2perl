#!/bin/bash

# Simple backtick usage test
echo "=== Basic Command Substitution ==="

# Simple command substitution
echo "Current date: `date +%Y`"
echo "Current directory: `pwd`"

# Assigning backtick results to variables
current_date=`date +%Y%m`
current_dir=`pwd`

echo "Stored date: $current_date"
echo "Stored directory: $current_dir"

# Simple function with backticks
get_file_size() {
    local file=$1
    local size=`wc -c < "$file"`
    echo "File $file has $size bytes"
}

get_file_size 000__04_basic_backtick_usage_simple.sh

echo "=== Basic Command Substitution Complete ==="
