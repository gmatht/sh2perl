#!/bin/bash
# Mission item 8: a command substitution whose body CANNOT constant-fold.
#
# The gate's own step 2 pointed at 000__02, whose two named captures fold to
# string literals — so the native-capture branch (`local *STDOUT` around the
# body) never ran in the gate at all, and a future "fix" to it would ship
# unexercised. Here the body depends on a positional parameter and on two
# builtins evaluated at run time, so no fold can produce it.
#
# The Perl must carry the native capture idiom, not a folded literal, and the
# captured bytes must still match bash.
# PERL_MUST_CONTAIN local *STDOUT
# PERL_MUST_NOT_CONTAIN "hi\nsecond\n"
greeting=${1:-hi}

x=$(echo "$greeting"; echo second)
echo "x=$x"

# Same shape inside a function, with a parameter the compiler cannot know.
show() {
    y=$(echo "$1"; echo tail-part)
    echo "y=$y"
}
show from-argument

# A multi-command body whose second command nests another capture.
z=$(echo "$greeting"; echo "inside=$(basename /x/y/sh2perl)")
echo "z=$z"

# A nested `$()` inside the captured body must be computed, not pasted.
n=$(echo "$(basename /a/b/c)")
echo "n=$n"
