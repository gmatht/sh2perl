#!/usr/bin/env bash
arr=(1 2 3)
sum=0
for x in "${arr[@]}"; do
  sum=$((sum + x))
done
echo "sum=$sum len=${#arr[@]}"
