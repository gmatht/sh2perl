# \$ escaping in interpolated strings: \$$n must yield $42 (the
# emitted template must not turn it into a literal $0 / lose the value).
n=42
echo "id: \$$n"
echo "price: \$5.00"
