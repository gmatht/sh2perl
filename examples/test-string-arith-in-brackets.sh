# $((...)) arithmetic INSIDE a [ ] test string must be evaluated before
# the test is tokenized (a[1]+a[2]=50; 1 < 50).
a=(10 20 30 40)
n=1
if [ "$n" -lt $(( a[1] + a[2] )) ]; then echo arith-test-ok; else echo arith-test-bad; fi
echo done
