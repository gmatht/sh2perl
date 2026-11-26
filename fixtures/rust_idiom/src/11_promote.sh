count=0
count=$((count + 1))
name="world"
if [[ "$name" == *orl* ]]; then
  echo "hi $name $count"
fi
total=0
for n in 1 2 3; do
  total=$((total + n))
done
echo "total=$total"
