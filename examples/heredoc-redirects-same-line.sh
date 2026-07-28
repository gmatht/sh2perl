#!/bin/sh
# Heredoc with additional redirects on the same line
(cmd) <<EOF 2>&1 >/dev/null
body
EOF
printf 'heredoc+redirects parsed OK\n'
