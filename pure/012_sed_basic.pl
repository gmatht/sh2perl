#!/usr/bin/perl

# Example 012: Basic sed command using system() and backticks
# This demonstrates the sed builtin called from Perl

print "=== Example 012: Basic sed command ===\n";

# Create test file first
open(my $fh, '>', 'test_sed.txt') or die "Cannot create test file: $!\n";
print $fh "This is line one\n";
print $fh "This is line two\n";
print $fh "This is line three\n";
print $fh "Another line with test\n";
print $fh "Final line\n";
close($fh);

# Simple sed substitution using backticks
print "Using backticks to call sed (substitute 'line' with 'LINE'):\n";

print $sed_output;

# sed with specific line numbers using system()
print "\nsed with specific line numbers (substitute only line 2):\n";
system("sed", "2s/line/LINE/", "test_sed.txt");

# sed with delete using backticks
print "\nsed with delete (delete line 3):\n";

print $sed_delete;

# sed with insert using system()
print "\nsed with insert (insert before line 2):\n";
system("sed", "2i\\INSERTED LINE", "test_sed.txt");

# sed with append using backticks
print "\nsed with append (append after line 2):\n";

print $sed_append;

# sed with print using system()
print "\nsed with print (print line 2):\n";
system("sed", "-n", "2p", "test_sed.txt");

# sed with multiple commands using backticks
print "\nsed with multiple commands:\n";

print $sed_multi;

# sed with in-place editing using system()
print "\nsed with in-place editing:\n";
system("cp", "test_sed.txt", "test_sed_backup.txt");
system("sed", "-i", "s/line/LINE/g", "test_sed.txt");
print "After in-place editing:\n";
system("cat", "test_sed.txt");

# sed with regular expressions using backticks
print "\nsed with regular expressions (substitute word boundaries):\n";

print $sed_regex;

# sed with case insensitive using system()
print "\nsed with case insensitive:\n";
system("sed", "s/line/LINE/gi", "test_sed_backup.txt");

# sed with global substitution using backticks
print "\nsed with global substitution:\n";

print $sed_global;

# sed from stdin using system() with echo
print "\nsed from stdin (echo | sed):\n";
system("echo 'This is a test line' | sed 's/test/TEST/'");

# sed with line ranges using backticks
print "\nsed with line ranges (substitute lines 2-4):\n";

print $sed_range;

# Clean up
unlink('test_sed.txt') if -f 'test_sed.txt';
unlink('test_sed_backup.txt') if -f 'test_sed_backup.txt';

print "=== Example 012 completed successfully ===\n";
