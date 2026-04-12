#!/usr/bin/perl

# Example 016: Basic sort command using system() and backticks
# This demonstrates the sort builtin called from Perl

print "=== Example 016: Basic sort command ===\n";

# Create test file first
open(my $fh, '>', 'test_sort.txt') or die "Cannot create test file: $!\n";
print $fh "Charlie\n";
print $fh "Alice\n";
print $fh "Bob\n";
print $fh "Diana\n";
print $fh "Eve\n";
close($fh);

# Create numeric test file
open(my $fh2, '>', 'test_sort_num.txt') or die "Cannot create test file: $!\n";
print $fh2 "25\n";
print $fh2 "10\n";
print $fh2 "5\n";
print $fh2 "30\n";
print $fh2 "15\n";
close($fh2);

# Simple sort using backticks
print "Using backticks to call sort (alphabetical):\n";

print $sort_output;

# sort with reverse using system()
print "\nsort with reverse (-r):\n";
system("sort", "-r", "test_sort.txt");

# sort with numeric using backticks
print "\nsort with numeric (-n):\n";

print $sort_num;

# sort with unique using system()
print "\nsort with unique (-u):\n";
system("sort", "-u", "test_sort.txt");

# sort with case insensitive using backticks
print "\nsort with case insensitive (-f):\n";

print $sort_case;

# sort with field separator using system()
print "\nsort with field separator (sort by second field):\n";
system("echo 'Alice,25\nBob,30\nCharlie,20' | sort -t',' -k2 -n");

# sort with multiple keys using backticks
print "\nsort with multiple keys:\n";
my $sort_multi = `echo 'Alice 25\nBob 30\nAlice 20' | sort -k1,1 -k2,2n`;
print $sort_multi;

# sort with human readable using system()
print "\nsort with human readable (-h):\n";
system("echo '1K\n2M\n500\n1G' | sort -h");

# sort with version using backticks
print "\nsort with version (-V):\n";

print $sort_version;

# sort with random using system()
print "\nsort with random (-R):\n";
system("sort", "-R", "test_sort.txt");

# sort from stdin using system() with echo
print "\nsort from stdin (echo | sort):\n";
system("echo 'Zebra\nApple\nBanana' | sort");

# sort with merge using backticks
print "\nsort with merge (-m):\n";

print $sort_merge;

# sort with check using system()
print "\nsort with check (-c):\n";
system("sort", "-c", "test_sort.txt");

# Clean up
unlink('test_sort.txt') if -f 'test_sort.txt';
unlink('test_sort_num.txt') if -f 'test_sort_num.txt';

print "=== Example 016 completed successfully ===\n";
