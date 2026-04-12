#!/usr/bin/perl

# Example 015: Basic cut command using system() and backticks
# This demonstrates the cut builtin called from Perl

print "=== Example 015: Basic cut command ===\n";

# Create test file first
open(my $fh, '>', 'test_cut.txt') or die "Cannot create test file: $!\n";
print $fh "Alice,25,95.5,Engineer\n";
print $fh "Bob,30,87.2,Manager\n";
print $fh "Charlie,35,92.8,Developer\n";
print $fh "Diana,28,88.9,Designer\n";
print $fh "Eve,32,91.3,Analyst\n";
close($fh);

# Simple cut with delimiter using backticks
print "Using backticks to call cut (cut by comma, field 1):\n";

print $cut_output;

# cut with multiple fields using system()
print "\ncut with multiple fields (fields 1 and 3):\n";
system("cut", "-d,", "-f1,3", "test_cut.txt");

# cut with range of fields using backticks
print "\ncut with range of fields (fields 1-3):\n";

print $cut_range;

# cut with character positions using system()
print "\ncut with character positions (characters 1-10):\n";
system("cut", "-c1-10", "test_cut.txt");

# cut with specific characters using backticks
print "\ncut with specific characters (characters 1,3,5):\n";
my $cut_chars = `cut -c1,3,5 test_cut.txt`;
print $cut_chars;

# cut with complement using system()
print "\ncut with complement (everything except field 2):\n";
system("cut", "-d,", "--complement", "-f2", "test_cut.txt");

# cut with output delimiter using backticks
print "\ncut with output delimiter:\n";
my $cut_od = `cut -d',' -f1,3 --output-delimiter=' | ' test_cut.txt`;
print $cut_od;

# cut with only delimited using system()
print "\ncut with only delimited (skip lines without delimiter):\n";
system("cut", "-d,", "-s", "-f1", "test_cut.txt");

# cut with bytes using backticks
print "\ncut with bytes (first 20 bytes):\n";

print $cut_bytes;

# cut with different delimiter using system()
print "\ncut with different delimiter (space):\n";
system("echo 'Alice 25 95.5 Engineer' | cut -d' ' -f1,3");

# cut from stdin using system() with echo
print "\ncut from stdin (echo | cut):\n";
system("echo 'John,40,85.5,Manager' | cut -d',' -f1,2");

# cut with field ranges using backticks
print "\ncut with field ranges (fields 2-4):\n";

print $cut_fields;

# cut with character ranges using system()
print "\ncut with character ranges (characters 5-15):\n";
system("cut", "-c5-15", "test_cut.txt");

# Clean up
unlink('test_cut.txt') if -f 'test_cut.txt';

print "=== Example 015 completed successfully ===\n";
