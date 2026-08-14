#!/bin/bash
# @ in test expressions
shopt -s extglob
x=foo
if [[ "$x" = @(foo|bar) ]]; then
    echo "matched"
fi
printf "parsed OK\\n"
