#!/bin/sh
# Heredoc with complex parameter expansions inside causes parse failure
if true; then
  cat <<EOF
${VAR} more ${VAR2} text
EOF
  echo done
fi
