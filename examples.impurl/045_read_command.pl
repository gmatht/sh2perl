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
my $read_output = `cat test_read_input.txt | head -1`;
print "First line: $read_output";

# read with variable assignment using system()
print "\nread with variable assignment (simulated):\n";
# shell 'read' won't populate Perl vars; simulate via sh -c and print
system("sh", "-c", "echo Hello World | { read VAR; echo Variable: \$VAR; }");

# read with multiple variables using backticks
print "\nread with multiple variables (simulated):\n";
my $read_multi = `echo "Alice 25 Engineer" | awk '{print "Name: " \$1 ", Age: " \$2 ", Role: " \$3}'`;
print $read_multi;

# read with delimiter using system()
print "\nread with delimiter (simulated):\n";
system('sh', ' -c', 'echo "Alice,25,Engineer" | awk -F, \'{print "Name: " $1 ", Age: " $2 ", Role: " $3}\'');

# read with timeout using backticks
print "\nread with timeout (simulated):\n";
my $read_timeout = `timeout 1 cat test_read_input.txt | head -1`;
print "Timeout result: $read_timeout";

# read with prompt using system()
print "\nread with prompt (simulated):\n";
system("sh", "-c", "printf '%s' 'Enter your name: '; echo 'John Doe'");

# read with silent input using backticks
print "\nread with silent input (simulated):\n";
my $read_silent = `echo "secret" | cat`;
print "Silent input: $read_silent";

# read with array using system()
print "\nread with array (simulated):\n";
system("sh", "-c", "echo 'Alice Bob Charlie' | awk '{for(i=1;i<=NF;i++) print \"Element \" i \": \" $i}'");

# read with file descriptor using backticks
print "\nread with file descriptor (simulated):\n";
my $read_fd = `cat test_read_input.txt | head -1`;
print "File descriptor result: $read_fd";

# read with error handling using system()
print "\nread with error handling:\n";
system("sh", "-c", "echo 'Error test' 2>/dev/null | cat");

# read with pipe using backticks
print "\nread with pipe:\n";
my $read_pipe = `cat test_read_input.txt | head -2`;
print $read_pipe;

# read with multiple lines using system()
print "\nread with multiple lines:\n";
system("sh", "-c", "cat test_read_input.txt | head -3");

# read with line counting using backticks
print "\nread with line counting:\n";
my $read_count = `cat test_read_input.txt | wc -l`;
print "Total lines: $read_count";

# read with character counting using system()
print "\nread with character counting:\n";
system("sh", "-c", "cat test_read_input.txt | wc -c");

# read with word counting using backticks
print "\nread with word counting:\n";
my $read_words = `cat test_read_input.txt | wc -w`;
print "Total words: $read_words";

# Clean up
unlink('test_read_input.txt') if -f 'test_read_input.txt';

print "=== Example 045 completed successfully ===\n";
