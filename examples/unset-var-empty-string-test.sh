# unset var must read as EMPTY (never as native 0): [ "$never_set" = "" ]
# and [ -z "$never_set2" ] — a lifted let initialised to 0 would
# defeat the empty-string test.
if [ "$never_set" = "" ]; then echo unset-is-empty; else echo unset-not-empty; fi
if [ -z "$never_set2" ]; then echo zero-len; fi
