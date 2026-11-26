if $(echo false); then echo yes; else echo no; fi
if $(echo true); then echo yes; else echo no; fi
if $(echo false) x; then echo yes; else echo no; fi
printf 'x\n' > f
if $(echo 'grep -q x') f; then echo gx; else echo gn; fi
echo $?
