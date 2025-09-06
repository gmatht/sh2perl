#!/usr/bin/perl

# Example 011: Basic grep command using system() and backticks
# This demonstrates the grep builtin called from Perl

print "=== Example 011: Basic grep command ===\n";

# Create test file first
open(my $fh, '>', 'test_grep.txt') or die "Cannot create test file: $!\n";
print $fh "This is line one\n";
print $fh "This is line two with the word test\n";
print $fh "This is line three\n";
print $fh "Another line with test in it\n";
print $fh "This line has no matches\n";
print $fh "Final line with test pattern\n";
close($fh);

# Simple grep command using backticks
print "Using backticks to call grep:\n";
my $grep_output = `grep test test_grep.txt`;
print $grep_output;

# grep with case insensitive using system()
print "\ngrep with case insensitive (-i):\n";
system("grep", "-i", "TEST", "test_grep.txt");

# grep with line numbers using backticks
print "\ngrep with line numbers (-n):\n";
my $grep_n = `grep -n test test_grep.txt`;
print $grep_n;

# grep with count using system()
print "\ngrep with count (-c):\n";
system("grep", "-c", "test", "test_grep.txt");

# grep with invert match using backticks
print "\ngrep with invert match (-v):\n";
my $grep_v = `grep -v test test_grep.txt`;
print $grep_v;

# grep with word match using system()
print "\ngrep with word match (-w):\n";
system("grep", "-w", "test", "test_grep.txt");

# grep with context using backticks
print "\ngrep with context (-C 1):\n";
my $grep_c = `grep -C 1 test test_grep.txt`;
print $grep_c;

# grep with before context using system()
print "\ngrep with before context (-B 2):\n";
system("grep", "-B", "2", "test", "test_grep.txt");

# grep with after context using backticks
print "\ngrep with after context (-A 2):\n";
my $grep_a = `grep -A 2 test test_grep.txt`;
print $grep_a;

# grep with extended regex using system()
print "\ngrep with extended regex (-E):\n";
system("grep", "-E", "test|line", "test_grep.txt");

# grep with fixed strings using backticks
print "\ngrep with fixed strings (-F):\n";
my $grep_f = `grep -F "test" test_grep.txt`;
print $grep_f;

# grep from stdin using system() with echo
print "\ngrep from stdin (echo | grep):\n";
system("echo 'This is a test line' | grep test");

# grep with multiple files using backticks
print "\ngrep with multiple files:\n";
my $grep_multi = `grep test test_grep.txt test_grep.txt`;
print $grep_multi;

# Clean up
unlink('test_grep.txt') if -f 'test_grep.txt';

print "=== Example 011 completed successfully ===\n";
