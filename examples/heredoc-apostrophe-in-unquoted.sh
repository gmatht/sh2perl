#!/bin/bash
# Heredoc with apostrophe in unquoted delimiter creates spanning string
cat << EOF
This is a test with an apostrophe: it's here.
EOF
echo "done"
