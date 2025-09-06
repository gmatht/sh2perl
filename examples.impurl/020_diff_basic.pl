#!/usr/bin/perl

# Example 020: Basic diff command using system() and backticks
# This demonstrates the diff builtin called from Perl

print "=== Example 020: Basic diff command ===\n";

# Create test files first
open(my $fh1, '>', 'test_diff1.txt') or die "Cannot create test file: $!\n";
print $fh1 "This is line one\n";
print $fh1 "This is line two\n";
print $fh1 "This is line three\n";
print $fh1 "This is line four\n";
print $fh1 "This is line five\n";
close($fh1);

open(my $fh2, '>', 'test_diff2.txt') or die "Cannot create test file: $!\n";
print $fh2 "This is line one\n";
print $fh2 "This is line two modified\n";
print $fh2 "This is line three\n";
print $fh2 "This is a new line\n";
print $fh2 "This is line five\n";
close($fh2);

# Simple diff using backticks
print "Using backticks to call diff:\n";
my $diff_output = `diff test_diff1.txt test_diff2.txt`;
print $diff_output;

# diff with unified format using system()
print "\ndiff with unified format (-u):\n";
system("diff", "-u", "test_diff1.txt", "test_diff2.txt");

# diff with context using backticks
print "\ndiff with context (-c):\n";
my $diff_context = `diff -c test_diff1.txt test_diff2.txt`;
print $diff_context;

# diff with side by side using system()
print "\ndiff with side by side (-y):\n";
system("diff", "-y", "test_diff1.txt", "test_diff2.txt");

# diff with ignore case using backticks
print "\ndiff with ignore case (-i):\n";
my $diff_ignore = `diff -i test_diff1.txt test_diff2.txt`;
print $diff_ignore;

# diff with ignore whitespace using system()
print "\ndiff with ignore whitespace (-w):\n";
system("diff", "-w", "test_diff1.txt", "test_diff2.txt");

# diff with ignore blank lines using backticks
print "\ndiff with ignore blank lines (-B):\n";
my $diff_blank = `diff -B test_diff1.txt test_diff2.txt`;
print $diff_blank;

# diff with ignore space change using system()
print "\ndiff with ignore space change (-b):\n";
system("diff", "-b", "test_diff1.txt", "test_diff2.txt");

# diff with recursive using backticks
print "\ndiff with recursive (-r):\n";
my $diff_recursive = `diff -r . . 2>/dev/null || echo "No differences found"`;
print $diff_recursive;

# diff with brief using system()
print "\ndiff with brief (-q):\n";
system("diff", "-q", "test_diff1.txt", "test_diff2.txt");

# diff with minimal using backticks
print "\ndiff with minimal (-d):\n";
my $diff_minimal = `diff -d test_diff1.txt test_diff2.txt`;
print $diff_minimal;

# diff with ignore all space using system()
print "\ndiff with ignore all space (-w):\n";
system("diff", "-w", "test_diff1.txt", "test_diff2.txt");

# diff from stdin using system() with echo
print "\ndiff from stdin (echo | diff):\n";
system("echo 'This is a test line' | diff - test_diff1.txt");

# Clean up
unlink('test_diff1.txt') if -f 'test_diff1.txt';
unlink('test_diff2.txt') if -f 'test_diff2.txt';

print "=== Example 020 completed successfully ===\n";
