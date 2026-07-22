#!/bin/bash

# For loop with glob and pipeline
for f in *.sh; do
    wc -l "$f" | cut -d' ' -f1
done | head -5
