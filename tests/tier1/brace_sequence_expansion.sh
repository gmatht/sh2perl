#!/bin/bash
# Brace SEQUENCES: bash expands a sequence only when it is the ENTIRE brace
# content; inside a comma list each part stays literal, and endpoints may be
# signed. One case per line, so a failure names the case it broke.
# expected (bash):
# {1..10}: 10 words -> 1 2 3 4 5 6 7 8 9 10
# {a..z}: 26 words -> a b c d e f g h i j k l m n o p q r s t u v w x y z
# {30..40}: 11 words -> 30 31 32 33 34 35 36 37 38 39 40
# file_{1..3}.txt: file_1.txt file_2.txt file_3.txt
# {1..10,20,30..40} (comma list stays literal): 1..10 20 30..40
# {-2..2}: -2 -1 0 1 2
# {3..-1}: 3 2 1 0 -1
# unsupported {a1..c3} stays literal: {a1..c3}
# unsupported {aa..cc} stays literal: {aa..cc}
# unsupported {a1..c3..2} keeps the step: {a1..c3..2}
# a comma list drops the braces: a1..c3 5
# {a..z..2}: a c e g i k m o q s u w y
# {10..1..3}: 10 7 4 1
# zero-padded {001..010..3}: 001 004 007 010
# step 0 means 1: {1..3..0} = 1 2 3
# comma list keeps the step text: 1..3..2 5
# nested {x{a1..c3},y}: x{a1..c3} y
# adjacent to a variable (x=VAR): braced var then range = VAR1 VAR2
# p{1..2}${x}q = p1VARq p2VARq
# braced var then alpha range = VARa VARb VARc
# bare-var brace list names x1/x2 (unset -> nothing): []
# alpha range then braced var = aVAR bVAR cVAR
# two braced vars around a range = VAR1YY VAR2YY
# x{1..2}y{3..4}z = x1y3z x1y4z x2y3z x2y4z
# unsupported range then braced var keeps the group: {a1..c3}VAR
# an assignment word does not brace-expand -> [VAR{1..2}]
# a plain assignment keeps its braces: w={a,b} -> [{a,b}]
# a quoted brace beside a variable stays literal: VAR{1..2}
set -- {1..10}
printf '{1..10}: %s words -> %s\n' "$#" "$*"
set -- {a..z}
printf '{a..z}: %s words -> %s\n' "$#" "$*"
set -- {30..40}
printf '{30..40}: %s words -> %s\n' "$#" "$*"
printf 'file_{1..3}.txt: %s\n' "$(echo file_{1..3}.txt)"
printf '{1..10,20,30..40} (comma list stays literal): %s\n' "$(echo {1..10,20,30..40})"
printf '{-2..2}: %s\n' "$(echo {-2..2})"
printf '{3..-1}: %s\n' "$(echo {3..-1})"

printf 'unsupported {a1..c3} stays literal: %s\n' "$(echo {a1..c3})"
printf 'unsupported {aa..cc} stays literal: %s\n' "$(echo {aa..cc})"
printf 'unsupported {a1..c3..2} keeps the step: %s\n' "$(echo {a1..c3..2})"
printf 'a comma list drops the braces: %s\n' "$(echo {a1..c3,5})"
printf '{a..z..2}: %s\n' "$(echo {a..z..2})"
printf '{10..1..3}: %s\n' "$(echo {10..1..3})"
printf 'zero-padded {001..010..3}: %s\n' "$(echo {001..010..3})"
printf 'step 0 means 1: {1..3..0} = %s\n' "$(echo {1..3..0})"
printf 'comma list keeps the step text: %s\n' "$(echo {1..3..2,5})"
printf 'nested {x{a1..c3},y}: %s\n' "$(echo {x{a1..c3},y})"

# ── Brace adjacent to a variable ──────────────────────────────────────────
# bash brace-expands the RAW WORD TEXT, so the pieces of `${x}{1..2}` are one
# word each: `${x}1` and `${x}2`. `$x{1,2}` names `$x1`/`$x2`.
x=VAR
y=YY
printf 'adjacent to a variable (x=VAR): braced var then range = %s\n' "$(echo ${x}{1..2})"
printf 'p{1..2}${x}q = %s\n' "$(echo p{1..2}${x}q)"
printf 'braced var then alpha range = %s\n' "$(echo ${x}{a..c})"
printf 'bare-var brace list names x1/x2 (unset -> nothing): [%s]\n' "$(echo $x{1,2})"
printf 'alpha range then braced var = %s\n' "$(echo {a..c}${x})"
printf 'two braced vars around a range = %s\n' "$(echo ${x}{1..2}${y})"
printf 'x{1..2}y{3..4}z = %s\n' "$(echo x{1..2}y{3..4}z)"
printf 'unsupported range then braced var keeps the group: %s\n' "$(echo {a1..c3}${x})"
v=${x}{1..2}
printf 'an assignment word does not brace-expand -> [%s]\n' "$v"
w={a,b}
printf 'a plain assignment keeps its braces: w={a,b} -> [%s]\n' "$w"
printf 'a quoted brace beside a variable stays literal: %s\n' "$(echo ${x}'{1..2}')"
