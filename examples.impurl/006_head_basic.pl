#!/usr/bin/perl

# Example 006: Basic head command using system() and backticks
# This demonstrates the head builtin called from Perl

print "=== Example 006: Basic head command ===\n";

# Create test file first
open(my $fh, '>', 'test_head.txt') or die "Cannot create test file: $!\n";
for my $i (1..10) {
    print $fh "Line $i: This is line number $i\n";
}
close($fh);

# Simple head command using backticks
print "Using backticks to call head (default 10 lines):\n";
my $head_output = `head test_head.txt`;
print $head_output;

# head -n 5 using system()
print "\nhead -n 5 (first 5 lines):\n";
system("head", "-n", "5", "test_head.txt");

# head -n 3 using backticks
print "\nhead -n 3 (first 3 lines):\n";
my $head3 = `head -n 3 test_head.txt`;
print $head3;

# head -n 1 using system()
print "\nhead -n 1 (first line only):\n";
system("head", "-n", "1", "test_head.txt");

# head with more lines than available using backticks
print "\nhead -n 15 (more than available):\n";
my $head15 = `head -n 15 test_head.txt`;
print $head15;

# head with bytes using system()
print "\nhead -c 50 (first 50 characters):\n";
system("head", "-c", "50", "test_head.txt");

# head with bytes using backticks
print "\nhead -c 100 (first 100 characters):\n";
my $head_bytes = `head -c 100 test_head.txt`;
print $head_bytes;

# head from stdin using system() with echo
print "\nhead from stdin (echo | head):\n";
system("echo 'Line 1\nLine 2\nLine 3\nLine 4\nLine 5' | head -n 3");

# head with quiet mode using backticks
print "\nhead -q (quiet mode, no filename):\n";
my $head_quiet = `head -q test_head.txt`;
print $head_quiet;

# head with verbose mode using system()
print "\nhead -v (verbose mode, with filename):\n";
system("head", "-v", "test_head.txt");

# Clean up
unlink('test_head.txt') if -f 'test_head.txt';

print "=== Example 006 completed successfully ===\n";
