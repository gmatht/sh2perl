# debug-overflow: bash wraps a 64-bit multiply; the generated program uses a
# plain `*`, so a DEBUG build panics (overflow-checks=on) while the -O build
# wraps exactly as bash does. `a` is built in a loop so no compiler can
# constant-fold it into the `arithmetic_overflow` lint.
a=0
i=0
while [ "$i" -lt 2 ]; do
  a=$((a + 2000000000))
  i=$((i + 1))
done
b=$((a * a))
echo "$b"
