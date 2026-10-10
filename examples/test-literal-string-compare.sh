#!/bin/bash
# A CONSTANT-RHS `[ ]` string test is a plain literal compare — the RHS
# cannot carry glob metachars, so the sh2 `=`-fallback pattern machinery
# must not fire (Rust backend: no __sh_glob_like/__sh_fnmatch for these).
if [ "a" = "a" ]; then echo eq; fi
if [ "a" != "a" ]; then echo bad; else echo ne; fi
if [ "a" = "b" ]; then echo bad; else echo not-equal; fi
# a run-time variable on the lhs is still fine with a constant rhs
x=hello
if [ "$x" = "hello" ]; then echo var-eq; fi
if [ "$x" != "other" ]; then echo var-ne; fi
# numeric-looking constants stay STRING compares (bash `=` is not `-eq`)
if [ "10" = "10" ]; then echo num-eq; fi
if [ "10" = "010" ]; then echo bad; else echo num-string-distinct; fi
