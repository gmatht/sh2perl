#!/bin/sh
# Minimal test: open3 with a builtin command name (like 'cp' or 'ps').
# The generator uses open3 which check_qx flags even in list form.
cp file1.txt file2.txt
ps aux
printf "parsed OK\\n"
