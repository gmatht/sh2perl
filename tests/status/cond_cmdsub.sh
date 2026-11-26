if $(echo false); then echo yes; else echo no; fi
if $(echo true); then echo yes; else echo no; fi
echo $?
