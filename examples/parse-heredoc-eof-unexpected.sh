#!/bin/sh
# Heredoc with parameter expansions inside
VAR=hello
VAR2=world
if true; then
  cat <<EOF
${VAR} more ${VAR2} text
EOF
fi
