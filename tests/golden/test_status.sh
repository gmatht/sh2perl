test -n abc
if [ $? -eq 0 ]; then echo yes; else echo no; fi
test -z abc
echo $?
