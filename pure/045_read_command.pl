#!/usr/bin/perl

# Example 045: read command using system() and backticks
# This demonstrates the read builtin called from Perl

print "=== Example 045: read command ===\n";

# Create test input file
open(my $fh, '>', 'test_read_input.txt') or die "Cannot create test file: $!\n";
print $fh "This is line one\n";
print $fh "This is line two\n";
print $fh "This is line three\n";
close($fh);

# Simple read using backticks
print "Using backticks to call read (simulated):\n";

print "First line: $read_output";

# read with variable assignment using system()
print "\nread with variable assignment (simulated):\n";
system("echo", "Hello World", "|", "read", "VAR", "&&", "echo", "Variable: $VAR");

# read with multiple variables using backticks
print "\nread with multiple variables (simulated):\n";

print $read_multi;

# read with delimiter using system()
print "\nread with delimiter (simulated):\n";
system("echo", "Alice,25,Engineer", "|", "awk", "-F,", "{print \"Name: \" $1 \", Age: \" $2 \", Role: \" $3}");

# read with timeout using backticks
print "\nread with timeout (simulated):\n";

print "Timeout result: $read_timeout";

# read with prompt using system()
print "\nread with prompt (simulated):\n";
system("echo", "-n", "Enter your name: ", "&&", "echo", "John Doe");

# read with silent input using backticks
print "\nread with silent input (simulated):\n";

print "Silent input: $read_silent";

# read with array using system()
print "\nread with array (simulated):\n";
system("echo", "Alice Bob Charlie", "|", "awk", "{for(i=1;i<=NF;i++) print \"Element \" i \": \" $i}");

# read with file descriptor using backticks
print "\nread with file descriptor (simulated):\n";

print "File descriptor result: $read_fd";

# read with error handling using system()
print "\nread with error handling:\n";
system("echo", "Error test", "2>/dev/null", "|", "cat");

# read with pipe using backticks
print "\nread with pipe:\n";

print $read_pipe;

# read with multiple lines using system()
print "\nread with multiple lines:\n";
system("cat", "test_read_input.txt", "|", "head", "-3");

# read with line counting using backticks
print "\nread with line counting:\n";

print "Total lines: $read_count";

# read with character counting using system()
print "\nread with character counting:\n";
system("cat", "test_read_input.txt", "|", "wc", "-c");

# read with word counting using backticks
print "\nread with word counting:\n";

print "Total words: $read_words";

# Clean up
unlink('test_read_input.txt') if -f 'test_read_input.txt';

print "=== Example 045 completed successfully ===\n";
