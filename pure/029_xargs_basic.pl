#!/usr/bin/perl

# Example 029: Basic xargs command using system() and backticks
# This demonstrates the xargs builtin called from Perl

print "=== Example 029: Basic xargs command ===\n";

# Create test files first
open(my $fh, '>', 'test_xargs_input.txt') or die "Cannot create test file: $!\n";
print $fh "file1.txt\n";
print $fh "file2.txt\n";
print $fh "file3.txt\n";
close($fh);

# Simple xargs using backticks
print "Using backticks to call xargs (echo each line):\n";

print $xargs_output;

# xargs with specific command using system()
print "\nxargs with specific command (ls):\n";
system("cat", "test_xargs_input.txt", "|", "xargs", "ls", "-l");

# xargs with multiple arguments using backticks
print "\nxargs with multiple arguments:\n";

print $xargs_multi;

# xargs with max arguments using system()
print "\nxargs with max arguments (-n 2):\n";
system("echo", "1 2 3 4 5", "|", "xargs", "-n", "2", "echo");

# xargs with delimiter using backticks
print "\nxargs with delimiter (-d ','):\n";

print $xargs_delim;

# xargs with null delimiter using system()
print "\nxargs with null delimiter (-0):\n";
system("printf", "file1.txt\\0file2.txt\\0file3.txt\\0", "|", "xargs", "-0", "echo");

# xargs with replace string using backticks
print "\nxargs with replace string (-I {}):\n";

print $xargs_replace;

# xargs with interactive using system()
print "\nxargs with interactive (-p):\n";
system("echo", "file1.txt file2.txt", "|", "xargs", "-p", "echo");

# xargs with verbose using backticks
print "\nxargs with verbose (-t):\n";

print $xargs_verbose;

# xargs with exit on error using system()
print "\nxargs with exit on error (-e):\n";
system("echo", "file1.txt nonexistent.txt file3.txt", "|", "xargs", "-e", "ls");

# xargs with max lines using backticks
print "\nxargs with max lines (-L 1):\n";

print $xargs_lines;

# xargs with parallel using system()
print "\nxargs with parallel (-P 2):\n";
system("echo", "1 2 3 4 5", "|", "xargs", "-P", "2", "-n", "1", "echo");

# xargs with no run if empty using backticks
print "\nxargs with no run if empty (-r):\n";

print "No output (empty input)\n";

# Clean up
unlink('test_xargs_input.txt') if -f 'test_xargs_input.txt';

print "=== Example 029 completed successfully ===\n";
