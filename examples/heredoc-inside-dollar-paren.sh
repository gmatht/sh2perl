#!/bin/sh
# Heredoc inside $(...) command substitution
# Failed: Unexpected end of input / ParenClose
x=$(cat <<EOF
line1
line2
EOF
)
echo done
