#!/bin/bash
# A construct must not change meaning because its generated text HAPPENS to
# look like a positional parameter or `$ARGV`.
#
# The printf numeric helper is generated Perl with regex captures; a text pass
# that rewrote bare `$N` as `$1 → ${__sh_args}[0]` converted the wrong string
# (realpath-cmdsub printed exit 0 where bash prints 1). The same pass could
# reach awk's own `$1` or a literal `$ARGV[0]` in a quoted string.
# expected (bash): 7|0|$ARGV[0] $1|awk:0
set --
set -- 7
printf '%d|' "$1"
n=08
printf '%d|' "$n" 2>/dev/null
printf '%s|' '$ARGV[0] $1'
awk 'BEGIN { printf "%s", "awk:" ($1 + 0) }'
echo
