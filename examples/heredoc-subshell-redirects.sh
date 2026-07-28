#!/bin/sh
# Heredoc inside subshell with redirects on same line
# Failed: Unexpected end of input (c2z pattern)
( eval "$var" ) <<EOF 2>&1 >/dev/null
$DATA
alias >! /tmp/file
set >! /tmp/file2
EOF
echo done
