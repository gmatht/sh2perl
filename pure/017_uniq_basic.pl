#!/usr/bin/perl

# Example 017: Basic uniq command using system() and backticks
# This demonstrates the uniq builtin called from Perl

print "=== Example 017: Basic uniq command ===\n";

# Create test file first
open(my $fh, '>', 'test_uniq.txt') or die "Cannot create test file: $!\n";
print $fh "apple\n";
print $fh "banana\n";
print $fh "apple\n";
print $fh "cherry\n";
print $fh "banana\n";
print $fh "apple\n";
print $fh "date\n";
print $fh "cherry\n";
close($fh);

# Simple uniq using backticks
print "Using backticks to call uniq:\n";

print $uniq_output;

# uniq with count using system()
print "\nuniq with count (-c):\n";
system("uniq", "-c", "test_uniq.txt");

# uniq with unique only using backticks
print "\nuniq with unique only (-u):\n";

print $uniq_u;

# uniq with duplicate only using system()
print "\nuniq with duplicate only (-d):\n";
system("uniq", "-d", "test_uniq.txt");

# uniq with ignore case using backticks
print "\nuniq with ignore case (-i):\n";

print $uniq_i;

# uniq with skip fields using system()
print "\nuniq with skip fields (-f 1):\n";
system("echo '1 apple\n2 banana\n1 apple\n3 cherry' | uniq -f 1");

# uniq with skip characters using backticks
print "\nuniq with skip characters (-s 2):\n";

print $uniq_s;

# uniq with check using system()
print "\nuniq with check (-c):\n";
system("uniq", "-c", "test_uniq.txt");

# uniq with all repeated using backticks
print "\nuniq with all repeated (-D):\n";

print $uniq_D;

# uniq with group using system()
print "\nuniq with group (-g):\n";
system("uniq", "-g", "test_uniq.txt");

# uniq from stdin using system() with echo
print "\nuniq from stdin (echo | uniq):\n";
system("echo 'apple\nbanana\napple\ncherry' | uniq");

# uniq with field separator using backticks
print "\nuniq with field separator:\n";

print $uniq_fs;

# uniq with width using system()
print "\nuniq with width (-w 3):\n";
system("echo 'abc\nabd\nabc\ndef' | uniq -w 3");

# Clean up
unlink('test_uniq.txt') if -f 'test_uniq.txt';

print "=== Example 017 completed successfully ===\n";
