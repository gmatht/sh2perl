# sync loop + direct function call — the callDirect/sync-loop roadmap
# (per-sample promise churn in a hot render loop).
f() { echo $(( $1 + $1 )); }
i=0
while [ $i -lt 5 ]; do
  f $i
  i=$(( i + 1 ))
done
