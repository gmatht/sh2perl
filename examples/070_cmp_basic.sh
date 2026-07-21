#!/bin/bash

# Basic cmp tests — compares two files byte by byte

# Setup
echo "hello" > /tmp/cmp_test_a.txt
echo "hello" > /tmp/cmp_test_b.txt
echo "world" > /tmp/cmp_test_c.txt
touch /tmp/cmp_test_empty.txt

# Identical files — exit 0, no output
cmp /tmp/cmp_test_a.txt /tmp/cmp_test_b.txt
echo "exit: $?"

# Different files — exit 1, reports first difference
cmp /tmp/cmp_test_a.txt /tmp/cmp_test_c.txt
echo "exit: $?"

# Compare with empty file
cmp /tmp/cmp_test_a.txt /tmp/cmp_test_empty.txt
echo "exit: $?"

# Both empty
cmp /tmp/cmp_test_empty.txt /tmp/cmp_test_empty.txt
echo "exit: $?"

# Silent mode (-s): no output, only exit code
cmp -s /tmp/cmp_test_a.txt /tmp/cmp_test_c.txt
echo "exit: $?"

# Process substitution
cmp <(echo a) <(echo a)
echo "aa exit: $?"
cmp <(echo b) <(echo c)
echo "bc exit: $?"

# Cleanup
rm -f /tmp/cmp_test_a.txt /tmp/cmp_test_b.txt /tmp/cmp_test_c.txt /tmp/cmp_test_empty.txt
