#!/bin/sh
# Dangling || after heredoc (no right operand)
cat >/tmp/file <<EOF ||
content
EOF
echo done
