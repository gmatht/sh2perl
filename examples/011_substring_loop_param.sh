#!/usr/bin/env bash
# Regression test: substring extraction inside a while loop, where the
# string arrives as a FUNCTION PARAMETER (the MIMEcroft draw_text shape
# — ${dt_t:$dt_i:1}). The loop counter must update every iteration so
# each character is read in turn; a LICM / loop-invariant hoist bug can
# lift the setVar("ch", slice(...)) out of the loop, making every
# iteration read index 0 (the first character) instead of the current
# index.
#
# The A1 shIR encodes the slice start as Str("$i") (a bare $var inside
# a DoubleQuoted string), not as a Var() node — the invariance analysis
# must decode it or it judges the slice loop-invariant and hoists it.
#
# expected (bash): HELLOWORLD
#   estree bug:    HHHHHHHHHH   (all first chars — slice hoisted)
print_chars() { s=$1
  i=0
  len=$2
  while [ "$i" -lt "$len" ]; do
    ch=${s:$i:1}
    printf "%s" "$ch"
    i=$((i + 1))
  done
  echo ""
}
print_chars "HELLOWORLD" 10
