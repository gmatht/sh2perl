#!/bin/bash
# Brace SEQUENCES, which bash expands only when the whole brace is one range.
#
#   {1..10}   -> ten words            {a..z}   -> twenty-six words
#   {30..40}  -> eleven words         {-2..2}  -> signed endpoints
#   {1..10,20,30..40} -> a COMMA list, so bash keeps each range LITERAL
#                        (`1..10 20 30..40`) — this is the shape
#                        examples/064_hard_to_generate.sh uses, and the gate
#                        compares stdout, so a "helpful" expansion of the
#                        ranges inside a list fails here.
# expected (bash): 10|1 2 3 4 5 6 7 8 9 10|26|11|file_1.txt file_2.txt file_3.txt|1..10 20 30..40|-2 -1 0 1 2
set -- {1..10}
n10=$#
seq10="$*"
set -- {a..z}
n26=$#
set -- {30..40}
n11=$#
files=$(echo file_{1..3}.txt)
literal=$(echo {1..10,20,30..40})
signed=$(echo {-2..2})
printf '%s|%s|%s|%s|%s|%s|%s\n' "$n10" "$seq10" "$n26" "$n11" "$files" "$literal" "$signed"
