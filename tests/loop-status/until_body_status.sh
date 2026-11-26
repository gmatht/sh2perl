i=0
until [ $i -ge 2 ]; do
  i=$((i + 1))
  false
done
echo $?
