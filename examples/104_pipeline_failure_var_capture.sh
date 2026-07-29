#!/bin/bash
# Variable capture from pipeline
files=$(ls /tmp 2>/dev/null | wc -l)
echo "Files in /tmp: $files"
count=$(echo "a b c" | wc -w)
echo "Words: $count"
echo "done"
