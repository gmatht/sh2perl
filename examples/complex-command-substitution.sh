#!/bin/sh
# Complex command substitution with subshells and redirects
result=$(
  (gzip -cdfq -- "$1" 4>&-; echo $? >&4) 3>&- |
    (gzip -cdfq -- "$2" 4>&-; echo $? >&4) 3>&- 5<&- </dev/null
)
echo done
