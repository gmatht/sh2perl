#!/usr/bin/perl

# Example 004: Basic cat command using system() and backticks
# This demonstrates the cat builtin called from Perl

print "=== Example 004: Basic cat command ===\n";

# Create test files first
open(my $fh1, '>', 'test_file1.txt') or die "Cannot create test file: $!\n";
print $fh1 "Line 1: This is test file 1\n";
print $fh1 "Line 2: Created for cat demonstration\n";
print $fh1 "Line 3: Multiple lines of content\n";
close($fh1);

open(my $fh2, '>', 'test_file2.txt') or die "Cannot create test file: $!\n";
print $fh2 "Line 1: This is test file 2\n";
print $fh2 "Line 2: Another file for cat demo\n";
print $fh2 "Line 3: Different content here\n";
close($fh2);

# Simple cat command using backticks
print "Using backticks to call cat:\n";
my $cat_output = `cat test_file1.txt`;
print $cat_output;

# cat with multiple files using system()
print "\ncat with multiple files using system():\n";
system("cat", "test_file1.txt", "test_file2.txt");

# cat with redirection using backticks
print "\ncat with redirection (cat file1 file2 > combined.txt):\n";
my $combined = `cat test_file1.txt test_file2.txt > combined.txt`;
if (-f "combined.txt") {
    print "Combined file created successfully\n";
    my $combined_content = `cat combined.txt`;
    print "Combined content:\n$combined_content";
}

# cat from stdin using system() with echo
print "\ncat from stdin (echo | cat):\n";
system("echo 'This is from stdin' | cat");

# cat with line numbers using backticks
print "\ncat with line numbers (cat -n):\n";
my $numbered = `cat -n test_file1.txt`;
print $numbered;

# cat with non-printing characters using system()
print "\ncat with non-printing characters (cat -v):\n";
system("cat", "-v", "test_file1.txt");

# cat with squeeze blank lines using backticks
print "\ncat with squeeze blank lines (cat -s):\n";
my $squeezed = `cat -s test_file1.txt`;
print $squeezed;

# cat with tabs using system()
print "\ncat with tabs (cat -T):\n";
system("cat", "-T", "test_file1.txt");

# Clean up
unlink('test_file1.txt') if -f 'test_file1.txt';
unlink('test_file2.txt') if -f 'test_file2.txt';
unlink('combined.txt') if -f 'combined.txt';

print "=== Example 004 completed successfully ===\n";
