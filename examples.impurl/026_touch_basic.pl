#!/usr/bin/perl

# Example 026: Basic touch command using system() and backticks
# This demonstrates the touch builtin called from Perl

print "=== Example 026: Basic touch command ===\n";

$ENV{TZ} = 'UTC';
$ENV{LC_ALL} = 'C';

# Create deterministic files with fixed timestamps.
print "Using system() to call touch (create file):\n";
system("touch", "-t", "202301011200", "test_touch_file.txt");
print -f "test_touch_file.txt" ? "File created successfully\n" : "File creation failed\n";

print "\ntouch with multiple files:\n";
my $touch_multi = `touch -t 202301011200 test_touch_file1.txt test_touch_file2.txt test_touch_file3.txt`;
print "Multiple files created successfully\n" if -f "test_touch_file1.txt" && -f "test_touch_file2.txt" && -f "test_touch_file3.txt";

print "\ntouch with verbose (-v):\n";
system("touch", "-v", "-t", "202301011200", "test_touch_verbose.txt");

print "\ntouch with no create (-c):\n";
my $touch_no_create = `touch -c test_touch_no_create.txt`;
print -f "test_touch_no_create.txt" ? "File already existed\n" : "File not created\n";

print "\ntouch with reference (-r):\n";
system("touch", "-r", "test_touch_file.txt", "test_touch_reference.txt");
print "File created with reference timestamp\n" if -f "test_touch_reference.txt";

print "\ntouch with specific time (-t 202301011200):\n";
my $touch_time = `touch -t 202301011200 test_touch_time.txt`;
print "File created with specific timestamp\n" if -f "test_touch_time.txt";

print "\ntouch with date (-d '2023-01-01 12:00:00'):\n";
system("touch", "-d", "2023-01-01 12:00:00", "test_touch_date.txt");
print "File created with specific date\n" if -f "test_touch_date.txt";

print "\ntouch with access time (-a):\n";
my $touch_access = `touch -a test_touch_file.txt`;
print "Access time updated\n";

print "\ntouch with modification time (-m):\n";
system("touch", "-m", "test_touch_file.txt");
print "Modification time updated\n";

print "\ntouch with both times (-a -m):\n";
my $touch_both = `touch -a -m test_touch_file.txt`;
print "Both access and modification times updated\n";

print "\ntouch with no dereference (-h):\n";
system("touch", "-h", "test_touch_no_deref.txt");
print "File created with no dereference\n" if -f "test_touch_no_deref.txt";

print "\ntouch with error handling:\n";
my $touch_error = `touch test_touch_error.txt 2>&1`;
print -f "test_touch_error.txt" ? "File created successfully\n" : "File creation failed\n";

print "\ntouch with specific mode (--mode=644):\n";
system("touch", "--mode=644", "test_touch_mode.txt");
print "File created with specific mode\n" if -f "test_touch_mode.txt";

# Clean up
unlink('test_touch_file.txt') if -f 'test_touch_file.txt';
unlink('test_touch_file1.txt') if -f 'test_touch_file1.txt';
unlink('test_touch_file2.txt') if -f 'test_touch_file2.txt';
unlink('test_touch_file3.txt') if -f 'test_touch_file3.txt';
unlink('test_touch_verbose.txt') if -f 'test_touch_verbose.txt';
unlink('test_touch_no_create.txt') if -f 'test_touch_no_create.txt';
unlink('test_touch_reference.txt') if -f 'test_touch_reference.txt';
unlink('test_touch_time.txt') if -f 'test_touch_time.txt';
unlink('test_touch_date.txt') if -f 'test_touch_date.txt';
unlink('test_touch_no_deref.txt') if -f 'test_touch_no_deref.txt';
unlink('test_touch_error.txt') if -f 'test_touch_error.txt';
unlink('test_touch_mode.txt') if -f 'test_touch_mode.txt';

print "=== Example 026 completed successfully ===\n";
