#!/bin/sh
# Heredoc with apostrophes in body
cat << 'EOF'
This is a test with an apostrophe: it's fine
EOF
echo done
