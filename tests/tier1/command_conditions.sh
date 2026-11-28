#!/bin/bash
# COMMAND CONDITIONS: a command in condition position means "exit status 0",
# and the emitted Perl must take the same branch bash does. Non-command
# conditions (`[ ]`, `[[ ]]`, arithmetic) must stay Perl-native expressions.
# One labelled line per shape, so a failure names the case it broke.
# expected (bash):
# if grep success: yes
# if grep failure: no
# if ! grep failure: ok
# if ! grep success: ok
# elsif native-then-command: b
# elsif command-then-native: e
# elsif command-then-command: h
# while test: 1
# while test: 2
# until test: 1
# until test: 2
# while command: 1
# pipeline condition: ok
# pipeline condition: ok2
# negated pipeline condition: ok
# elsif pipeline: ok
# and condition: ok
# or condition: ok
# and of commands: ok
# or of commands: ok
# [-f]: file
# [-f negated]: ok
# [ -eq ]: ok
# [[ glob ]]: ok
# [[ regex ]]: ok
# function true: ok
# function false: ok
# cd failure: ok
# cd success: ok
# true condition: ok
# false condition: ok
# subshell true: ok
# subshell false: ok
# ternary arith: 10
# ternary arith zero: 10
# ternary arith false: 20
# subshell status: 3
# status after failed if: 0
# status after true if: 0
if grep -q root /etc/passwd; then echo "if grep success: yes"; else echo "if grep success: NO"; fi
if grep -q zzznotthere /etc/passwd; then echo "if grep failure: YES"; else echo "if grep failure: no"; fi
if ! grep -q zzznotthere /etc/passwd; then echo "if ! grep failure: ok"; else echo "if ! grep failure: BAD"; fi
if ! grep -q root /etc/passwd; then echo "if ! grep success: BAD"; else echo "if ! grep success: ok"; fi
if [ 1 -eq 2 ]; then echo "a"; elif grep -q root /etc/passwd; then echo "elsif native-then-command: b"; else echo "elsif native-then-command: c"; fi
if grep -q zzz /etc/passwd; then echo "d"; elif [ 1 -eq 1 ]; then echo "elsif command-then-native: e"; else echo "f"; fi
if grep -q zzz /etc/passwd; then echo "g"; elif grep -q root /etc/passwd; then echo "elsif command-then-command: h"; else echo "i"; fi
n=0
while [ "$n" -lt 2 ]; do n=$((n+1)); echo "while test: $n"; done
m=0
until [ "$m" -ge 2 ]; do m=$((m+1)); echo "until test: $m"; done
touch cond-flag.txt
i=0
while ! grep -q x cond-flag.txt 2>/dev/null; do echo x >> cond-flag.txt; i=$((i+1)); echo "while command: $i"; done
if echo hi | grep -q hi; then echo "pipeline condition: ok"; else echo "pipeline condition: BAD"; fi
if echo hi | grep -q zz; then echo "pipeline condition: BAD2"; else echo "pipeline condition: ok2"; fi
if ! echo hi | grep -q zz; then echo "negated pipeline condition: ok"; else echo "negated pipeline condition: BAD"; fi
if echo hi | grep -q zz; then echo "e"; elif echo hi | grep -q hi; then echo "elsif pipeline: ok"; else echo "f"; fi
if true && false; then echo "and condition: BAD"; else echo "and condition: ok"; fi
if false || true; then echo "or condition: ok"; else echo "or condition: BAD"; fi
if grep -q root /etc/passwd && grep -q zzz /etc/passwd; then echo "and of commands: BAD"; else echo "and of commands: ok"; fi
if grep -q zzz /etc/passwd || grep -q root /etc/passwd; then echo "or of commands: ok"; else echo "or of commands: BAD"; fi
if [ -f /etc/passwd ]; then echo "[-f]: file"; else echo "[-f]: NOFILE"; fi
if [ ! -f /etc/definitely-not-here ]; then echo "[-f negated]: ok"; fi
if [ 1 -eq 1 ]; then echo "[ -eq ]: ok"; fi
if [[ abc == a* ]]; then echo "[[ glob ]]: ok"; fi
if [[ abc =~ ^a ]]; then echo "[[ regex ]]: ok"; fi
f() { return 0; }
g() { return 1; }
if f; then echo "function true: ok"; fi
if g; then echo "function false: BAD"; else echo "function false: ok"; fi
d=/definitely/not/here
if cd "$d" 2>/dev/null; then echo "cd failure: BAD"; else echo "cd failure: ok"; fi
if cd /tmp; then echo "cd success: ok"; fi
if true; then echo "true condition: ok"; fi
if false; then echo "false condition: BAD"; else echo "false condition: ok"; fi
if (exit 0); then echo "subshell true: ok"; fi
if (exit 1); then echo "subshell false: BAD"; else echo "subshell false: ok"; fi
k=5
echo "ternary arith: $(( k > 3 ? k * 2 : k ))"
j=1
echo "ternary arith zero: $(( j ? 10 : 20 ))"
z=0
echo "ternary arith false: $(( z ? 10 : 20 ))"
if (exit 3); then echo "subshell status: BAD"; else echo "subshell status: $?"; fi
if grep -q zzz /etc/passwd; then :; fi; echo "status after failed if: $?"
if grep -q root /etc/passwd; then :; fi; echo "status after true if: $?"
rm -f cond-flag.txt
