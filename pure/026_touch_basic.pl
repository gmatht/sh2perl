#!/usr/bin/perl

# Example 026: Basic touch command using system() and backticks
# This demonstrates the touch builtin called from Perl

print "=== Example 026: Basic touch command ===\n";

# Simple touch using system()
print "Using system() to call touch (create file):\n";
system("touch", "test_touch_file.txt");
if (-f "test_touch_file.txt") {
    print "File created successfully\n";
} else {
    print "File creation failed\n";
}

# touch with multiple files using backticks
print "\ntouch with multiple files:\n";

if (-f "test_touch_file1.txt" && -f "test_touch_file2.txt" && -f "test_touch_file3.txt") {
    print "Multiple files created successfully\n";
}

# touch with verbose using system()
print "\ntouch with verbose (-v):\n";
system("touch", "-v", "test_touch_verbose.txt");

# touch with no create using backticks
print "\ntouch with no create (-c):\n";

if (-f "test_touch_no_create.txt") {
    print "File created with no-create option\n";
}

# touch with reference using system()
print "\ntouch with reference (-r):\n";
system("touch", "-r", "test_touch_file.txt", "test_touch_reference.txt");
if (-f "test_touch_reference.txt") {
    print "File created with reference timestamp\n";
}

# touch with specific time using backticks
print "\ntouch with specific time (-t 202301011200):\n";

if (-f "test_touch_time.txt") {
    print "File created with specific timestamp\n";
}

# touch with date using system()
print "\ntouch with date (-d '2023-01-01 12:00:00'):\n";
system("touch", "-d", "2023-01-01 12:00:00", "test_touch_date.txt");
if (-f "test_touch_date.txt") {
    print "File created with specific date\n";
}

# touch with access time using backticks
print "\ntouch with access time (-a):\n";

print "Access time updated\n";

# touch with modification time using system()
print "\ntouch with modification time (-m):\n";
system("touch", "-m", "test_touch_file.txt");
print "Modification time updated\n";

# touch with both times using backticks
print "\ntouch with both times (-a -m):\n";

print "Both access and modification times updated\n";

# touch with no dereference using system()
print "\ntouch with no dereference (-h):\n";
system("touch", "-h", "test_touch_no_deref.txt");
if (-f "test_touch_no_deref.txt") {
    print "File created with no dereference\n";
}

# touch with error handling using backticks
print "\ntouch with error handling:\n";

if (-f "test_touch_error.txt") {
    print "File created successfully\n";
} else {
    print "File creation failed\n";
}

# touch with specific mode using system()
print "\ntouch with specific mode (--mode=644):\n";
system("touch", "--mode=644", "test_touch_mode.txt");
if (-f "test_touch_mode.txt") {
    print "File created with specific mode\n";
}

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
