#!/bin/sh
# Nested $() containing a pipeline with subshell and redirects.
# Tests that capture_parenthetical_text correctly handles the
# combination of $(), (), |, and >&- redirects.
r=$(
    exec 4>&1
    (echo "test" 4>&-; echo $? >&4) 3>&- |
        cat >&3 4>&-
) || {
    x=$?
}
echo "$r"
printf "%s=[%s]\n" x "${x:-}"

