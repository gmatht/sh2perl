#!/bin/bash
# Process substitution output >(...) used as redirect target
LOG_FILE=$(mktemp /tmp/proc_subst_test.XXXXXX)
# Save original stdout
exec 3>&1
exec 1> >(tee -a "$LOG_FILE")
exec 2> >(tee -a "$LOG_FILE" >&2)
echo "test output" >&2
# Restore stdout and show result
exec 1>&3 3>&-
printf 'process substitution log file content=[%s]\n' "$(cat "$LOG_FILE" 2>/dev/null)"
rm -f "$LOG_FILE"
