#!/bin/sh
# Minimal test: open3 with a builtin command name (like 'cp' or 'cat').
# The generator uses open3 which check_qx flags even in list form.
echo "hello world"
echo "done: $?"
