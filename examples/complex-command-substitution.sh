#!/bin/sh
# Complex command substitution with subshells and redirects
# (the original fd-racing form is replaced by a deterministic nested
# cmdsub over real files; the fd-redirect parse pin lives in
# parse-dollar-paren-pipe.sh)
d=$(mktemp -d)
printf 'hi' | gzip > "$d/f1"
printf 'there' | gzip > "$d/f2"
result=$(
  gzip -cdfq -- "$d/f1" 2>/dev/null
  gzip -cdfq -- "$d/f2" 2>/dev/null
)
printf 'result=%s\n' "$result"
rm -rf "$d"
