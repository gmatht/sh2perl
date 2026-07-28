#!/bin/sh
# Heredoc with additional redirects on the same line
# Parse error: Unexpected end of input (for <<EOF with 2>&1 >/dev/null)
(cmd) <<EOF 2>&1 >/dev/null
body
EOF
echo done
