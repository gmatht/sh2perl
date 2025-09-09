#!/bin/bash
echo "Complex test script"
for i in {1..5}; do
    echo "Iteration $i"
    if [ $i -eq 3 ]; then
        echo "Special case at iteration 3"
    fi
done
find . -name "*.rs" -type f | head -3 | while read file; do
    echo "Found: $file"
done
echo "Script completed"
