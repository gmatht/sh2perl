#!/usr/bin/env bash
# Collect example .sh scripts excluding archived examples
# Emits a newline-separated list of example paths under examples/

set -eu -o pipefail

root_dir=$(git rev-parse --show-toplevel 2>/dev/null || pwd)
cd "$root_dir"

# Find .sh files under examples/, excluding the archived directory.
# Exclude files in examples/archived/ and only include regular files ending with .sh
find examples -type f -name "*.sh" \
  -not -path "examples/archived/*" \
  -print | sort
