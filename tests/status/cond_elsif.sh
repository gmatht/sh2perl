printf 'x\n' > f
if false; then
  echo a
elif grep -q z f; then
  echo b
elif grep -q x f; then
  echo c
else
  echo d
fi
echo $?
