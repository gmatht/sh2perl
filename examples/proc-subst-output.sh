#!/bin/bash
LOG_FILE="/tmp/proc_subst_test.txt"
exec 3>&1
exec 1> >(tee -a "$LOG_FILE")
echo "test output"
exec 1>&3 3>&-
printf "log: []\n" "$(cat "$LOG_FILE" 2>/dev/null)"
rm -f "$LOG_FILE"
