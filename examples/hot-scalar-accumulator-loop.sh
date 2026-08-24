# hot scalar accumulator loop — the typed-local lifting target
# (si_*/tf_*-style hot vars become plain lets; cross-chunk vars stay
# store-backed).
s=0
i=0
while [ $i -lt 100 ]; do
  s=$(( s + i ))
  i=$(( i + 1 ))
done
echo "sum=$s"
