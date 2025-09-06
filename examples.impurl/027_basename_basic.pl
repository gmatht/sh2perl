#!/usr/bin/perl

# Example 027: Basic basename command using system() and backticks
# This demonstrates the basename builtin called from Perl

print "=== Example 027: Basic basename command ===\n";

# Simple basename using backticks
print "Using backticks to call basename:\n";
my $basename_output = `basename /path/to/file.txt`;
print "basename /path/to/file.txt: $basename_output";

# basename with suffix using system()
print "\nbasename with suffix (remove .txt):\n";
system("basename", "/path/to/file.txt", ".txt");

# basename with multiple suffixes using backticks
print "\nbasename with multiple suffixes:\n";
my $basename_multi = `basename /path/to/file.txt .txt .bak`;
print "basename /path/to/file.txt .txt .bak: $basename_multi";

# basename with zero suffix using system()
print "\nbasename with zero suffix (-s ''):\n";
system("basename", "-s", "", "/path/to/file.txt");

# basename with multiple paths using backticks
print "\nbasename with multiple paths:\n";
my $basename_paths = `basename /path/to/file1.txt /path/to/file2.txt /path/to/file3.txt`;
print "Multiple paths: $basename_paths";

# basename with directory using system()
print "\nbasename with directory:\n";
system("basename", "/home/user/documents/");

# basename with current directory using backticks
print "\nbasename with current directory:\n";
my $basename_current = `basename .`;
print "Current directory: $basename_current";

# basename with parent directory using system()
print "\nbasename with parent directory:\n";
system("basename", "..");

# basename with root directory using backticks
print "\nbasename with root directory:\n";
my $basename_root = `basename /`;
print "Root directory: $basename_root";

# basename with empty string using system()
print "\nbasename with empty string:\n";
system("basename", "");

# basename with relative path using backticks
print "\nbasename with relative path:\n";
my $basename_relative = `basename ../file.txt`;
print "Relative path: $basename_relative";

# basename with hidden file using system()
print "\nbasename with hidden file:\n";
system("basename", "/path/to/.hidden.txt");

# basename with file without extension using backticks
print "\nbasename with file without extension:\n";
my $basename_no_ext = `basename /path/to/file`;
print "File without extension: $basename_no_ext";

# basename with multiple extensions using system()
print "\nbasename with multiple extensions:\n";
system("basename", "/path/to/file.txt.bak", ".txt.bak");

# Clean up
print "=== Example 027 completed successfully ===\n";
