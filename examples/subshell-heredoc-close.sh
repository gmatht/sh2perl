#!/bin/sh
# Subshell with heredoc followed by closing paren
# Failed: Unexpected token: ParenClose (git-filter-branch pattern)
(
  cat <<EOF
content
EOF
)
eval "$var"
echo done
