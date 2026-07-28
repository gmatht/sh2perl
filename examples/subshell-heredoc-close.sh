#!/bin/sh
# Subshell with heredoc followed by closing paren
var=hello
(
  cat <<EOF
content
EOF
)
eval "$var"
printf 'subshell+heredoc+eval parsed OK\n'
