#!/usr/bin/perl

# Example 021: Basic find command using system() and backticks
# This demonstrates the find builtin called from Perl

print "=== Example 021: Basic find command ===\n";

# Create test directory structure
system("mkdir", "-p", "test_find_dir/subdir1");
system("mkdir", "-p", "test_find_dir/subdir2");
system("touch", "test_find_dir/file1.txt");
system("touch", "test_find_dir/file2.pl");
system("touch", "test_find_dir/subdir1/file3.txt");
system("touch", "test_find_dir/subdir2/file4.sh");

# Simple find using backticks
print "Using backticks to call find (all files):\n";

print $find_output;

# find with name pattern using system()
print "\nfind with name pattern (*.txt):\n";
system("find", "test_find_dir", "-name", "*.txt");

# find with type directory using backticks
print "\nfind with type directory (-type d):\n";

print $find_dirs;

# find with size using system()
print "\nfind with size (files larger than 0 bytes):\n";
system("find", "test_find_dir", "-size", "+0c");

# find with mtime using backticks
print "\nfind with mtime (modified in last 1 day):\n";

print $find_mtime;

# find with maxdepth using system()
print "\nfind with maxdepth (max depth 1):\n";
system("find", "test_find_dir", "-maxdepth", "1");

# find with mindepth using backticks
print "\nfind with mindepth (min depth 2):\n";

print $find_mindepth;

# find with exec using system()
print "\nfind with exec (list file details):\n";
system("find", "test_find_dir", "-name", "*.txt", "-exec", "ls", "-l", "{}", ";");

# find with print using backticks
print "\nfind with print (-print):\n";

print $find_print;

# find with iname using system()
print "\nfind with iname (case insensitive):\n";
system("find", "test_find_dir", "-iname", "*.TXT");

# find with empty using backticks
print "\nfind with empty (empty files):\n";

print $find_empty;

# find with newer using system()
print "\nfind with newer (newer than file1.txt):\n";
system("find", "test_find_dir", "-newer", "test_find_dir/file1.txt");

# find with perm using backticks
print "\nfind with perm (executable files):\n";

print $find_perm;

# find with user using system()
print "\nfind with user (current user):\n";

chomp $current_user;
system("find", "test_find_dir", "-user", $current_user);

# Clean up
system("rm", "-rf", "test_find_dir");

print "=== Example 021 completed successfully ===\n";
