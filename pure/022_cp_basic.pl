#!/usr/bin/perl

# Example 022: Basic cp command using system() and backticks
# This demonstrates the cp builtin called from Perl

print "=== Example 022: Basic cp command ===\n";

# Create test files first
open(my $fh, '>', 'test_cp_source.txt') or die "Cannot create test file: $!\n";
print $fh "This is a test file for copying\n";
print $fh "It has multiple lines\n";
print $fh "To demonstrate cp functionality\n";
close($fh);

# Create test directory
system("mkdir", "-p", "test_cp_dir");

# Simple cp using system()
print "Using system() to call cp (copy file):\n";
system("cp", "test_cp_source.txt", "test_cp_dest.txt");
if (-f "test_cp_dest.txt") {
    print "File copied successfully\n";
    
    print "Content: $content";
}

# cp with recursive using system()
print "\ncp with recursive (-r):\n";
system("cp", "-r", "test_cp_dir", "test_cp_dir_copy");
if (-d "test_cp_dir_copy") {
    print "Directory copied successfully\n";
}

# cp with preserve using backticks
print "\ncp with preserve (-p):\n";

if (-f "test_cp_preserve.txt") {
    print "File copied with preserve attributes\n";
}

# cp with verbose using system()
print "\ncp with verbose (-v):\n";
system("cp", "-v", "test_cp_source.txt", "test_cp_verbose.txt");

# cp with force using backticks
print "\ncp with force (-f):\n";

if (-f "test_cp_force.txt") {
    print "File copied with force\n";
}

# cp with interactive using system()
print "\ncp with interactive (-i):\n";
system("cp", "-i", "test_cp_source.txt", "test_cp_interactive.txt");

# cp with update using backticks
print "\ncp with update (-u):\n";

if (-f "test_cp_update.txt") {
    print "File copied with update\n";
}

# cp with backup using system()
print "\ncp with backup (-b):\n";
system("cp", "-b", "test_cp_source.txt", "test_cp_backup.txt");

# cp with suffix using backticks
print "\ncp with suffix (--suffix=.bak):\n";

if (-f "test_cp_suffix.txt") {
    print "File copied with suffix\n";
}

# cp with multiple files using system()
print "\ncp with multiple files:\n";
system("cp", "test_cp_source.txt", "test_cp_source2.txt", "test_cp_dir/");

# cp with preserve all using backticks
print "\ncp with preserve all (-a):\n";

if (-f "test_cp_all.txt") {
    print "File copied with preserve all\n";
}

# cp with no dereference using system()
print "\ncp with no dereference (-P):\n";
system("cp", "-P", "test_cp_source.txt", "test_cp_no_deref.txt");

# Clean up
unlink('test_cp_source.txt') if -f 'test_cp_source.txt';
unlink('test_cp_dest.txt') if -f 'test_cp_dest.txt';
unlink('test_cp_preserve.txt') if -f 'test_cp_preserve.txt';
unlink('test_cp_verbose.txt') if -f 'test_cp_verbose.txt';
unlink('test_cp_force.txt') if -f 'test_cp_force.txt';
unlink('test_cp_interactive.txt') if -f 'test_cp_interactive.txt';
unlink('test_cp_update.txt') if -f 'test_cp_update.txt';
unlink('test_cp_backup.txt') if -f 'test_cp_backup.txt';
unlink('test_cp_suffix.txt') if -f 'test_cp_suffix.txt';
unlink('test_cp_source2.txt') if -f 'test_cp_source2.txt';
unlink('test_cp_all.txt') if -f 'test_cp_all.txt';
unlink('test_cp_no_deref.txt') if -f 'test_cp_no_deref.txt';
system("rm", "-rf", "test_cp_dir");
system("rm", "-rf", "test_cp_dir_copy");

print "=== Example 022 completed successfully ===\n";
