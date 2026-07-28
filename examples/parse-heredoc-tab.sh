#!/bin/sh
# Test: heredoc with tabs <<-
cat <<-EOF
	indented content
	with tabs
EOF
echo "done"
