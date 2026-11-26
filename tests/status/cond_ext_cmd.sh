printf 'x\n' > f
if grep -q x f; then echo yes; else echo no; fi
echo $?
if grep -q z f; then echo yes; else echo no; fi
echo $?
