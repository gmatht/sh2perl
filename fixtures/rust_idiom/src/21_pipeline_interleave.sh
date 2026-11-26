#!/usr/bin/env bash
# Pipelines the emitted program must wire itself, and shell output that has to
# interleave with a child's INHERITED fd 1 in bash's order: `printf` without a
# trailing newline leaves text in Rust's line buffer, so the buffer must be
# flushed before the child is spawned or the child's bytes jump ahead.
printf 'start:'
cat alpha.txt | tr a-z A-Z | wc -l
printf 'flush:'
printf '%s\n' one two three | sort -r | head -2
printf 'glob:'
wc -l alpha.txt beta.txt gamma.txt | grep total
two=$(printf '%s\n' a b c | sed -n 2p)
echo "two=$two"
echo "head:$(printf '%s\n' x y z | tail -1)"
lastcats=$(cat alpha.txt beta.txt | wc -l)
echo "lines=$lastcats"
# a stage carrying a redirection cannot be spelled as an argv: it keeps the
# documented `bash -c` fallback (see native_stage_argv), and must still match
grep -h . alpha.txt beta.txt > joined.txt | wc -c
wc -l < joined.txt
printf 'end\n'
