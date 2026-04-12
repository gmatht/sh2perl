#!/usr/bin/perl

# Example 019: Basic comm command using system() and backticks
# This demonstrates the comm builtin called from Perl

print "=== Example 019: Basic comm command ===\n";

# Create test files first
open(my $fh1, '>', 'test_comm1.txt') or die "Cannot create test file: $!\n";
print $fh1 "apple\n";
print $fh1 "banana\n";
print $fh1 "cherry\n";
print $fh1 "date\n";
print $fh1 "elderberry\n";
close($fh1);

open(my $fh2, '>', 'test_comm2.txt') or die "Cannot create test file: $!\n";
print $fh2 "banana\n";
print $fh2 "cherry\n";
print $fh2 "fig\n";
print $fh2 "grape\n";
print $fh2 "elderberry\n";
close($fh2);

# Sort files first (comm requires sorted input)
system("sort", "test_comm1.txt", "-o", "test_comm1_sorted.txt");
system("sort", "test_comm2.txt", "-o", "test_comm2_sorted.txt");

# Simple comm using backticks
print "Using backticks to call comm:\n";

print $comm_output;

# comm with suppress column 1 using system()
print "\ncomm with suppress column 1 (-1):\n";
system("comm", "-1", "test_comm1_sorted.txt", "test_comm2_sorted.txt");

# comm with suppress column 2 using backticks
print "\ncomm with suppress column 2 (-2):\n";

print $comm_2;

# comm with suppress column 3 using system()
print "\ncomm with suppress column 3 (-3):\n";
system("comm", "-3", "test_comm1_sorted.txt", "test_comm2_sorted.txt");

# comm with suppress columns 1 and 2 using backticks
print "\ncomm with suppress columns 1 and 2 (-12):\n";

print $comm_12;

# comm with suppress columns 1 and 3 using system()
print "\ncomm with suppress columns 1 and 3 (-13):\n";
system("comm", "-13", "test_comm1_sorted.txt", "test_comm2_sorted.txt");

# comm with suppress columns 2 and 3 using backticks
print "\ncomm with suppress columns 2 and 3 (-23):\n";

print $comm_23;

# comm with suppress all columns using system()
print "\ncomm with suppress all columns (-123):\n";
system("comm", "-123", "test_comm1_sorted.txt", "test_comm2_sorted.txt");

# comm with delimiter using backticks
print "\ncomm with delimiter (-d ','):\n";

print $comm_delim;

# comm with check using system()
print "\ncomm with check (check if files are sorted):\n";
system("comm", "--check-order", "test_comm1_sorted.txt", "test_comm2_sorted.txt");

# comm with total using backticks
print "\ncomm with total (-t):\n";

print $comm_total;

# comm with zero delimiter using system()
print "\ncomm with zero delimiter (-z):\n";
system("comm", "-z", "test_comm1_sorted.txt", "test_comm2_sorted.txt");

# Clean up
unlink('test_comm1.txt') if -f 'test_comm1.txt';
unlink('test_comm2.txt') if -f 'test_comm2.txt';
unlink('test_comm1_sorted.txt') if -f 'test_comm1_sorted.txt';
unlink('test_comm2_sorted.txt') if -f 'test_comm2_sorted.txt';

print "=== Example 019 completed successfully ===\n";
