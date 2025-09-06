#!/usr/bin/perl

# Example 007: Basic tail command using system() and backticks
# This demonstrates the tail builtin called from Perl

print "=== Example 007: Basic tail command ===\n";

# Create test file first
open(my $fh, '>', 'test_tail.txt') or die "Cannot create test file: $!\n";
for my $i (1..10) {
    print $fh "Line $i: This is line number $i\n";
}
close($fh);

# Simple tail command using backticks
print "Using backticks to call tail (default 10 lines):\n";
my $tail_output = `tail test_tail.txt`;
print $tail_output;

# tail -n 5 using system()
print "\ntail -n 5 (last 5 lines):\n";
system("tail", "-n", "5", "test_tail.txt");

# tail -n 3 using backticks
print "\ntail -n 3 (last 3 lines):\n";
my $tail3 = `tail -n 3 test_tail.txt`;
print $tail3;

# tail -n 1 using system()
print "\ntail -n 1 (last line only):\n";
system("tail", "-n", "1", "test_tail.txt");

# tail with more lines than available using backticks
print "\ntail -n 15 (more than available):\n";
my $tail15 = `tail -n 15 test_tail.txt`;
print $tail15;

# tail with bytes using system()
print "\ntail -c 50 (last 50 characters):\n";
system("tail", "-c", "50", "test_tail.txt");

# tail with bytes using backticks
print "\ntail -c 100 (last 100 characters):\n";
my $tail_bytes = `tail -c 100 test_tail.txt`;
print $tail_bytes;

# tail from stdin using system() with echo
print "\ntail from stdin (echo | tail):\n";
system("echo 'Line 1\nLine 2\nLine 3\nLine 4\nLine 5' | tail -n 3");

# tail with follow mode using backticks (simulated)
print "\ntail -f simulation (follow mode):\n";
my $tail_follow = `tail -n 3 test_tail.txt`;
print $tail_follow;

# tail with quiet mode using system()
print "\ntail -q (quiet mode, no filename):\n";
system("tail", "-q", "test_tail.txt");

# Clean up
unlink('test_tail.txt') if -f 'test_tail.txt';

print "=== Example 007 completed successfully ===\n";
