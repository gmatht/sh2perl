#!/usr/bin/perl

# Example 005: Basic wc command using system() and backticks
# This demonstrates the wc builtin called from Perl

print "=== Example 005: Basic wc command ===\n";

# Create test file first
open(my $fh, '>', 'test_wc.txt') or die "Cannot create test file: $!\n";
print $fh "This is line one\n";
print $fh "This is line two\n";
print $fh "This is line three\n";
print $fh "Fourth line with more words\n";
print $fh "Fifth and final line\n";
close($fh);

# Simple wc command using backticks
print "Using backticks to call wc:\n";
my $wc_output = `wc test_wc.txt`;
print $wc_output;

# wc with specific options using system()
print "\nwc -l (line count only):\n";
system("wc", "-l", "test_wc.txt");

print "\nwc -w (word count only):\n";
system("wc", "-w", "test_wc.txt");

print "\nwc -c (character count only):\n";
system("wc", "-c", "test_wc.txt");

# wc with multiple files using backticks
print "\nwc with multiple files:\n";
my $multi_wc = `wc test_wc.txt test_wc.txt`;
print $multi_wc;

# wc from stdin using system() with echo
print "\nwc from stdin (echo | wc):\n";
system("echo 'This is a test line' | wc");

# wc with bytes using backticks
print "\nwc -c (bytes):\n";
my $bytes = `wc -c test_wc.txt`;
print $bytes;

# wc with maximum line length using system()
print "\nwc -L (maximum line length):\n";
system("wc", "-L", "test_wc.txt");

# wc with all options using backticks
print "\nwc -lwc (lines, words, characters):\n";
my $all_wc = `wc -lwc test_wc.txt`;
print $all_wc;

# wc on multiple files with totals using system()
print "\nwc with totals on multiple files:\n";
system("wc", "test_wc.txt", "test_wc.txt");

# Clean up
unlink('test_wc.txt') if -f 'test_wc.txt';

print "=== Example 005 completed successfully ===\n";
