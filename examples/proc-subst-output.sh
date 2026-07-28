#!/bin/bash
# Process substitution output >(...) used as redirect target
exec 1> >(tee -a "$LOG_FILE")
exec 2> >(tee -a "$LOG_FILE" >&2)
echo "done"
