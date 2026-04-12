#!/usr/bin/perl

# Example 013: Basic awk command using system() and backticks
# This demonstrates the awk builtin called from Perl

print "=== Example 013: Basic awk command ===\n";

# Create test file first
open(my $fh, '>', 'test_awk.txt') or die "Cannot create test file: $!\n";
print $fh "Alice 25 95.5\n";
print $fh "Bob 30 87.2\n";
print $fh "Charlie 35 92.8\n";
print $fh "Diana 28 88.9\n";
print $fh "Eve 32 91.3\n";
close($fh);

# Simple awk print using backticks
print "Using backticks to call awk (print all lines):\n";

print $awk_output;

# awk print specific fields using system()
print "\nawk print specific fields (print first and third field):\n";
system("awk", "{print $1, $3}", "test_awk.txt");

# awk with field separator using backticks
print "\nawk with field separator (print with comma separator):\n";

print $awk_fs;

# awk with conditions using system()
print "\nawk with conditions (print lines where age > 30):\n";
system("awk", "$2 > 30 {print $1, $2}", "test_awk.txt");

# awk with calculations using backticks
print "\nawk with calculations (print name and score*2):\n";

print $awk_calc;

# awk with BEGIN and END using system()
print "\nawk with BEGIN and END:\n";
system("awk", "BEGIN{print \"Name\\tAge\\tScore\"} {print $1\"\\t\"$2\"\\t\"$3} END{print \"End of data\"}", "test_awk.txt");

# awk with variables using backticks
print "\nawk with variables (sum of scores):\n";

print $awk_sum;

# awk with string functions using system()
print "\nawk with string functions (uppercase names):\n";
system("awk", "{print toupper($1), $2, $3}", "test_awk.txt");

# awk with pattern matching using backticks
print "\nawk with pattern matching (names starting with A):\n";

print $awk_pattern;

# awk with multiple conditions using system()
print "\nawk with multiple conditions (age > 30 AND score > 90):\n";
system("awk", "$2 > 30 && $3 > 90 {print $1, $2, $3}", "test_awk.txt");

# awk with formatting using backticks
print "\nawk with formatting (printf):\n";

print $awk_printf;

# awk from stdin using system() with echo
print "\nawk from stdin (echo | awk):\n";
system("echo 'John 40 85.5' | awk '{print $1, $2, $3}'");

# awk with field width using backticks
print "\nawk with field width:\n";

print $awk_width;

# Clean up
unlink('test_awk.txt') if -f 'test_awk.txt';

print "=== Example 013 completed successfully ===\n";
