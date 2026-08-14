# tsv = tsv + ... per sample — the chunked-building roadmap target
# (array push + join instead of per-iteration string concat).
tsv=""
for i in 1 2 3 4 5; do
  tsv="$tsv$i,"
done
echo "$tsv"
