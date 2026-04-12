#!/usr/bin/perl

# Example 014: Basic tr command using system() and backticks
# This demonstrates the tr builtin called from Perl

print "=== Example 014: Basic tr command ===\n";

# Create test file first
open(my $fh, '>', 'test_tr.txt') or die "Cannot create test file: $!\n";
print $fh "Hello World\n";
print $fh "This is a test\n";
print $fh "UPPERCASE TEXT\n";
print $fh "lowercase text\n";
print $fh "Mixed Case Text\n";
close($fh);

# Simple tr translation using backticks
print "Using backticks to call tr (translate a to A):\n";

print $tr_output;

# tr with case conversion using system()
print "\ntr with case conversion (lowercase to uppercase):\n";
system("tr", "a-z", "A-Z", "test_tr.txt");

# tr with delete using backticks
print "\ntr with delete (delete all spaces):\n";

print $tr_delete;

# tr with complement using system()
print "\ntr with complement (delete all non-letters):\n";
system("tr", "-cd", "a-zA-Z", "test_tr.txt");

# tr with squeeze using backticks
print "\ntr with squeeze (squeeze multiple spaces):\n";

print $tr_squeeze;

# tr with character classes using system()
print "\ntr with character classes (delete digits):\n";
system("tr", "-d", "[:digit:]", "test_tr.txt");

# tr with multiple characters using backticks
print "\ntr with multiple characters (translate vowels):\n";

print $tr_vowels;

# tr with ranges using system()
print "\ntr with ranges (translate a-z to A-Z):\n";
system("tr", "a-z", "A-Z", "test_tr.txt");

# tr with complement and delete using backticks
print "\ntr with complement and delete (keep only letters):\n";

print $tr_keep;

# tr with squeeze and translate using system()
print "\ntr with squeeze and translate:\n";
system("tr", "-s", "a-z", "A-Z", "test_tr.txt");

# tr from stdin using system() with echo
print "\ntr from stdin (echo | tr):\n";
system("echo 'Hello World' | tr 'a-z' 'A-Z'");

# tr with specific characters using backticks
print "\ntr with specific characters (translate l to L):\n";

print $tr_specific;

# tr with character sets using system()
print "\ntr with character sets (translate punctuation):\n";
system("tr", "[:punct:]", "X", "test_tr.txt");

# Clean up
unlink('test_tr.txt') if -f 'test_tr.txt';

print "=== Example 014 completed successfully ===\n";
