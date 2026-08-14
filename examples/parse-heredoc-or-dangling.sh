#!/bin/sh
# Dangling || after heredoc (no right operand)
f=$(mktemp)
cat >"$f" <<EOF
content
EOF
printf 'wrote %d bytes\n' $(wc -c < "$f")
rm -f "$f"
