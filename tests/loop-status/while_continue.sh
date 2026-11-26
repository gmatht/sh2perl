n=0
while [ $n -lt 3 ]; do
  n=$((n + 1))
  if [ $n -eq 2 ]; then continue; fi
  echo $n
done
echo $?
