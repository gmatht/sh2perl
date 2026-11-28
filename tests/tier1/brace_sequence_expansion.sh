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
