# Every shell builtin used as a COMMAND WORD must lower to native Perl in
# every position — plain statement, if/elsif/while/until condition, negated
# `!`, and either side of `&&`/`||` — and a FAILING builtin must leave its
# status behind while the script keeps running (bash never dies on a failed
# cd/rm/mkdir/touch/printf).
#
# PERL_MUST_NOT_CONTAIN: system(
# PERL_MUST_NOT_CONTAIN: bash', '-c'
# PERL_MUST_NOT_CONTAIN: or die
# PERL_MUST_NOT_CONTAIN: die "

cd /tmp
echo $?
cd /no/such
echo $?
if [ $? -eq 0 ]; then echo aft-cd-ok; else echo aft-cd-bad; fi

echo hi
echo $?
printf '%s\n' pf-line
printf '%d\n' zz
echo $?
true
echo $?
false
echo $?
if true; then echo true-ok; else echo true-bad; fi
if false; then echo false-bad; else echo false-ok; fi

test -n abc
echo $?
test -n ''
echo $?
[ -n abc ]
echo $?
[ -z '' ]
echo $?

rm -f /tmp/sh2_builtin_missing_xyz
echo $?
rm /no/such/dir/f
echo $?
mkdir /no/such/dir
echo $?
mkdir -p /tmp/sh2_builtin_dir
echo $?
touch /tmp/sh2_builtin_dir/f
echo $?
touch /no/such/dir/t
echo $?

if cd /tmp; then echo cd-if-ok; else echo cd-if-bad; fi
if cd /no/such; then echo cd-if-bad; else echo cd-if-fail-ok; fi
if ! cd /no/such; then echo cd-neg-ok; else echo cd-neg-bad; fi
if echo echoed; then echo echo-cond-ok; else echo echo-cond-bad; fi
if printf '%s\n' pfc; then echo printf-cond-ok; fi
if test -n abc; then echo test-cond-ok; else echo test-cond-bad; fi
if [ -z '' ]; then echo bracket-cond-ok; else echo bracket-cond-bad; fi
if rm -f /tmp/sh2_builtin_missing_xyz; then echo rm-cond-ok; fi
if mkdir -p /tmp/sh2_builtin_dir2; then echo mkdir-cond-ok; fi
if touch /tmp/sh2_builtin_dir2/f; then echo touch-cond-ok; fi
if false; then echo a; elif cd /no/such; then echo b; elif cd /tmp; then echo elsif-ok; else echo elsif-bad; fi

n=0
while cd /no/such; do n=1; break; done
echo while-cd=$n
n=0
until cd /tmp; do n=1; break; done
echo until-cd=$n
n=0
while test -n abc; do n=$((n + 1)); break; done
echo while-test=$n

cd /no/such && echo and-bad || echo and-ok
cd /tmp && echo and-ok
rm -f /tmp/sh2_builtin_missing_xyz && echo rm-and-ok
mkdir -p /tmp/sh2_builtin_dir3 && echo mkdir-and-ok
touch /tmp/sh2_builtin_dir3/f && echo touch-and-ok
test -n abc && echo test-and-ok
[ -z '' ] && echo bracket-and-ok
false || echo or-ok
echo $?

exit 0
