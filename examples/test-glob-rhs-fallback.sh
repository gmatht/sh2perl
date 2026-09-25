#!/bin/bash
# bash `[ ]` RHS is ALWAYS a literal string — never a glob — so a constant
# `"a*"` compares literally, and so does a variable holding metachars.
# (Globbing needs `[[ ]]` with an unquoted pattern; that path currently
# trips an unrelated Perl-backend gap, tracked separately.)
if [ "abc" = "a*" ]; then echo bad; else echo literal-star-nomatch; fi
if [ "xyz" = "a*" ]; then echo bad; else echo const-star-nomatch; fi
pat='a*'
if [ "abc" = "$pat" ]; then echo bad; else echo var-star-nomatch; fi
# a quoted pattern in [[ ]] is literal too
if [[ "abc" == "a*" ]]; then echo bad; else echo db-bracket-nomatch; fi
# a literal character class in [ ] is still a literal compare
if [ "abc" = "[ab]*" ]; then echo bad; else echo bracket-literal-nomatch; fi
