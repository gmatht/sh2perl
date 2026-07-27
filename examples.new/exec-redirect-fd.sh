#!/bin/sh
# Demonstrates exec with file descriptor copy redirect
exec 0<&9
echo "done"
