if cd /no/such; then echo yes; else echo no; fi
echo $?
if cd /tmp; then echo yes; else echo no; fi
echo $?
if echo hi; then echo yes; else echo no; fi
echo $?
if false; then echo yes; else echo no; fi
echo $?
