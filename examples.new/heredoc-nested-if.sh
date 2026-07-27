#!/bin/sh
# Demonstrates heredoc inside nested if/else/fi
if [ -n "$VAR" ] && [ -z "$OTHER" ]; then
    if ! command --flag 'pattern' arg 2>/dev/null | grep -q 'test'; then
        cat >&2 <<EOF
warning: $NAME is missing
EOF
    fi
fi
echo "done"
