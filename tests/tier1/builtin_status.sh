#!/bin/bash
# NATIVE BUILTIN STATUS: a builtin lowered to native Perl (chdir, unlink,
# rmdir, mkdir, utime, Perl prints, test/[[ ]], true/false, exit) must leave
# the shell status bash leaves, on both the success and the failure path.
# A failed builtin must not die, and a successful one must not inherit an
# earlier child's status. One labelled line per shape.
# expected (bash):
# cd-fail rc=1
# cd-ok rc=0
# if-ok
# still-running
# rm-missing rc=0
# rm-ok rc=0
# rm-rf rc=0
# rm-noargs rc=1
# mkdir-fail rc=1
# mkdir-ok rc=0
# mkdir-exists rc=1
# mkdir-exists-detected
# touch-ok rc=0
# touch-fail rc=1
# hi
# echo rc=0
# x
# printf rc=0
# echo-after-false rc=0
# printf-after-false rc=0
# rmdir-fail rc=1
# rmdir-ok rc=0
# test-true rc=0
# test-false rc=1
# dbl-true rc=0
# dbl-false rc=1
# test-f rc=0
# test-f-missing rc=1
# true rc=0
# false rc=1
# true-after-false rc=0
# while-body-false rc=1
# until-body-true rc=0
# subshell-exit rc=4
# cd-after-false rc=0
# echo-after-false-2 rc=0
# true-after-false-2 rc=0
# test-after-false rc=0
# still here
# still here 2
# still here 3
# still here 4
# elif-cd-ok
# elif-test-ok
# bang-subshell ok
# bang-subshell2 ok
# bang-and-statement ran

# cd: failure reads as failure, success reads as 0, script keeps running.
cd /no/such/dir 2>/dev/null; echo "cd-fail rc=$?"
mkdir -p d1 && cd d1; echo "cd-ok rc=$?"
cd /no/such/dir 2>/dev/null
if [ $? -eq 0 ]; then echo "if-bad"; else echo "if-ok"; fi
echo "still-running"

# rm / rmdir
rm -f /no/such/file.txt 2>/dev/null; echo "rm-missing rc=$?"
touch gone.txt; rm -f gone.txt; echo "rm-ok rc=$?"
rm -rf /no/such/dir 2>/dev/null; echo "rm-rf rc=$?"
rm no-such-file 2>/dev/null; echo "rm-noargs rc=$?"

# mkdir: failure, success, and failure again read in a following `if`.
mkdir /no/such/parent/dir 2>/dev/null; echo "mkdir-fail rc=$?"
mkdir made; echo "mkdir-ok rc=$?"
mkdir made 2>/dev/null; mk=$?; echo "mkdir-exists rc=$mk"
if [ $mk -ne 0 ]; then echo "mkdir-exists-detected"; fi

# touch
touch newfile; echo "touch-ok rc=$?"
touch /no/such/dir/f 2>/dev/null; echo "touch-fail rc=$?"

# echo / printf (success after success and after an earlier failure)
echo hi; echo "echo rc=$?"
printf 'x\n'; echo "printf rc=$?"
false; echo hi >/dev/null; echo "echo-after-false rc=$?"
false; printf 'y\n' >/dev/null; echo "printf-after-false rc=$?"

# rmdir
rmdir /no/such/dir 2>/dev/null; echo "rmdir-fail rc=$?"
mkdir rd; rmdir rd; echo "rmdir-ok rc=$?"

# test / [ ] / [[ ]]
[ 1 -eq 1 ]; echo "test-true rc=$?"
[ 1 -eq 2 ]; echo "test-false rc=$?"
[[ abc == a* ]]; echo "dbl-true rc=$?"
[[ abc == z* ]]; echo "dbl-false rc=$?"
test -f /etc/passwd; echo "test-f rc=$?"
test -f /nope; echo "test-f-missing rc=$?"

# true / false
true; echo "true rc=$?"
false; echo "false rc=$?"
false; true; echo "true-after-false rc=$?"

# status inside loop bodies
n=0
while [ $n -lt 2 ]; do n=$((n+1)); false; done
echo "while-body-false rc=$?"
m=0
until [ $m -ge 1 ]; do m=$((m+1)); true; done
echo "until-body-true rc=$?"

# exit inside a subshell keeps the script alive (exit in THIS shell would end
# it before anything else could be observed)
( exit 4 ); echo "subshell-exit rc=$?"

# a successful builtin after an earlier failing command must read 0
false; cd /tmp; echo "cd-after-false rc=$?"
false; echo ok >/dev/null; echo "echo-after-false-2 rc=$?"
false; true; echo "true-after-false-2 rc=$?"
false; [ 1 -eq 1 ]; echo "test-after-false rc=$?"
cd - >/dev/null 2>&1

# a failed builtin must not stop the script
cd /no/such/dir 2>/dev/null; echo "still here"
rm -f /no/such/file 2>/dev/null; echo "still here 2"
mkdir /no/such/x 2>/dev/null; echo "still here 3"
touch /no/such/x 2>/dev/null; echo "still here 4"

# $? read in a following elif branch
d=/no/such/elif
if cd /tmp 2>/dev/null; then echo "elif-cd-ok"; elif [ $? -eq 0 ]; then echo "elif-bad"; fi
false; [ 1 -eq 2 ]
if false; then echo "elif-bad2"; elif [ 1 -eq 1 ]; then echo "elif-test-ok"; fi

# the two `!` shapes: a negated SUBSHELL in condition position, and `!` in
# statement position binding to one command (not the whole `&&` chain).
if ! (exit 1); then echo "bang-subshell ok"; else echo "bang-subshell bad"; fi
if ! (exit 0); then echo "bang-subshell2 bad"; else echo "bang-subshell2 ok"; fi
! false && echo "bang-and-statement ran"
! true && echo "bang-and-statement2 BAD"

cd /tmp
rm -rf builtin-scratch
