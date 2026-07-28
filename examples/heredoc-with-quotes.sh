#!/bin/sh
# Regression test: heredoc containing single quotes, which must not
# confuse the lexer into opening a false string.
cat <<'EOF'
I can't stop this feeling
EOF
echo "done"
