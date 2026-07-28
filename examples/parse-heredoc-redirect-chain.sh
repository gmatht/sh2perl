#!/bin/sh
# Test heredoc followed by more redirects on same line
cat <<EOF 2>&1 >/dev/null
hello
EOF
printf 'heredoc+redirect-chain parsed OK\n'
