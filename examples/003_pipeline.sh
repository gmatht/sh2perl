#!/bin/bash

# Pipeline examples
ls | grep "\.txt$" | wc -l
cat file.txt | sort | uniq -c | sort -nr
find . -name "*.sh" | xargs grep -l "function"  | tr -d "\\\\/"

# This pipeline will use line-by-line processing:
cat file.txt | tr 'a' 'b' | grep 'hello'

# This pipeline will fall back to buffered processing:
cat file.txt | sort | grep 'hello'