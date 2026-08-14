# arith with array-element expressions: $(( arr[i] * 2 )) — the
# emitter half of the native-arith roadmap (runtime side already landed).
arr=(10 20 30)
i=1
echo $(( arr[i] * 2 ))
echo $(( arr[0] + arr[2] ))
echo $(( (arr[1] + arr[2]) & 50 ))
