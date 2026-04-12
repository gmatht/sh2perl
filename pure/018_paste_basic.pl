#!/usr/bin/perl

# Example 018: Basic paste command using system() and backticks
# This demonstrates the paste builtin called from Perl

print "=== Example 018: Basic paste command ===\n";

# Create test files first
open(my $fh1, '>', 'test_paste1.txt') or die "Cannot create test file: $!\n";
print $fh1 "Alice\n";
print $fh1 "Bob\n";
print $fh1 "Charlie\n";
close($fh1);

open(my $fh2, '>', 'test_paste2.txt') or die "Cannot create test file: $!\n";
print $fh2 "25\n";
print $fh2 "30\n";
print $fh2 "35\n";
close($fh2);

open(my $fh3, '>', 'test_paste3.txt') or die "Cannot create test file: $!\n";
print $fh3 "Engineer\n";
print $fh3 "Manager\n";
print $fh3 "Developer\n";
close($fh3);

# Simple paste using backticks
print "Using backticks to call paste (two files):\n";

print $paste_output;

# paste with custom delimiter using system()
print "\npaste with custom delimiter (-d ','):\n";
system("paste", "-d", ",", "test_paste1.txt", "test_paste2.txt");

# paste with multiple files using backticks
print "\npaste with multiple files:\n";

print $paste_multi;

# paste with serial using system()
print "\npaste with serial (-s):\n";
system("paste", "-s", "test_paste1.txt");

# paste with newline delimiter using backticks
print "\npaste with newline delimiter (-d '\\n'):\n";

print $paste_nl;

# paste with tab delimiter using system()
print "\npaste with tab delimiter (-d '\\t'):\n";
system("paste", "-d", "\t", "test_paste1.txt", "test_paste2.txt");

# paste with zero delimiter using backticks
print "\npaste with zero delimiter (-d '\\0'):\n";

print $paste_zero;

# paste with space delimiter using system()
print "\npaste with space delimiter (-d ' '):\n";
system("paste", "-d", " ", "test_paste1.txt", "test_paste2.txt");

# paste with pipe delimiter using backticks
print "\npaste with pipe delimiter (-d '|'):\n";

print $paste_pipe;

# paste from stdin using system() with echo
print "\npaste from stdin (echo | paste):\n";
system("echo 'Alice\nBob' | paste - test_paste2.txt");

# paste with multiple delimiters using backticks
print "\npaste with multiple delimiters:\n";

print $paste_multi_delim;

# paste with serial and delimiter using system()
print "\npaste with serial and delimiter (-s -d ','):\n";
system("paste", "-s", "-d", ",", "test_paste1.txt");

# Clean up
unlink('test_paste1.txt') if -f 'test_paste1.txt';
unlink('test_paste2.txt') if -f 'test_paste2.txt';
unlink('test_paste3.txt') if -f 'test_paste3.txt';

print "=== Example 018 completed successfully ===\n";
