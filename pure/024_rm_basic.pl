#!/usr/bin/perl

# Example 024: Basic rm command using system() and backticks
# This demonstrates the rm builtin called from Perl

print "=== Example 024: Basic rm command ===\n";

# Create test files first
open(my $fh, '>', 'test_rm_file1.txt') or die "Cannot create test file: $!\n";
print $fh "This is a test file for removal\n";
close($fh);

open(my $fh2, '>', 'test_rm_file2.txt') or die "Cannot create test file: $!\n";
print $fh2 "This is another test file\n";
close($fh2);

# Create test directory
system("mkdir", "-p", "test_rm_dir");
system("touch", "test_rm_dir/file3.txt");

# Simple rm using system()
print "Using system() to call rm (remove file):\n";
system("rm", "test_rm_file1.txt");
if (!-f "test_rm_file1.txt") {
    print "File removed successfully\n";
} else {
    print "File removal failed\n";
}

# rm with verbose using system()
print "\nrm with verbose (-v):\n";
system("rm", "-v", "test_rm_file2.txt");

# rm with force using backticks
print "\nrm with force (-f):\n";

print "Force removal attempted\n";

# rm with interactive using system()
print "\nrm with interactive (-i):\n";
system("touch", "test_rm_interactive.txt");
system("rm", "-i", "test_rm_interactive.txt");

# rm with recursive using backticks
print "\nrm with recursive (-r):\n";

if (!-d "test_rm_dir") {
    print "Directory removed recursively\n";
}

# rm with recursive and force using system()
print "\nrm with recursive and force (-rf):\n";
system("mkdir", "-p", "test_rm_dir2/subdir");
system("touch", "test_rm_dir2/file.txt");
system("touch", "test_rm_dir2/subdir/file2.txt");
system("rm", "-rf", "test_rm_dir2");

# rm with preserve root using backticks
print "\nrm with preserve root (--preserve-root):\n";

print $rm_preserve;

# rm with one file system using system()
print "\nrm with one file system (-x):\n";
system("mkdir", "-p", "test_rm_xfs");
system("rm", "-x", "test_rm_xfs");

# rm with no dereference using backticks
print "\nrm with no dereference (-P):\n";

print $rm_no_deref;

# rm with ignore missing using system()
print "\nrm with ignore missing (-f):\n";
system("rm", "-f", "nonexistent_file.txt");
print "Ignored missing file\n";

# rm with directory using backticks
print "\nrm with directory (-d):\n";
system("mkdir", "-p", "test_rm_empty_dir");

if (!-d "test_rm_empty_dir") {
    print "Empty directory removed\n";
}

# rm with multiple files using system()
print "\nrm with multiple files:\n";
system("touch", "test_rm_multi1.txt");
system("touch", "test_rm_multi2.txt");
system("touch", "test_rm_multi3.txt");
system("rm", "test_rm_multi1.txt", "test_rm_multi2.txt", "test_rm_multi3.txt");
print "Multiple files removed\n";

# Clean up any remaining files
unlink('test_rm_file1.txt') if -f 'test_rm_file1.txt';
unlink('test_rm_file2.txt') if -f 'test_rm_file2.txt';
unlink('test_rm_interactive.txt') if -f 'test_rm_interactive.txt';
system("rm", "-rf", "test_rm_dir") if -d 'test_rm_dir';
system("rm", "-rf", "test_rm_dir2") if -d 'test_rm_dir2';
system("rm", "-rf", "test_rm_xfs") if -d 'test_rm_xfs';
system("rm", "-rf", "test_rm_empty_dir") if -d 'test_rm_empty_dir';

print "=== Example 024 completed successfully ===\n";
