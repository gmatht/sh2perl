#!/usr/bin/perl

# Example 027: Basic basename command using system() and backticks
# This demonstrates the basename builtin called from Perl

print "=== Example 027: Basic basename command ===\n";

# Simple basename using backticks
print "Using backticks to call basename:\n";

print "basename /path/to/file.txt: $basename_output";

# basename with suffix using system()
print "\nbasename with suffix (remove .txt):\n";
system("basename", "/path/to/file.txt", ".txt");

# basename with multiple suffixes using backticks
print "\nbasename with multiple suffixes:\n";

print "basename /path/to/file.txt .txt .bak: $basename_multi";

# basename with zero suffix using system()
print "\nbasename with zero suffix (-s ''):\n";
system("basename", "-s", "", "/path/to/file.txt");

# basename with multiple paths using backticks
print "\nbasename with multiple paths:\n";

print "Multiple paths: $basename_paths";

# basename with directory using system()
print "\nbasename with directory:\n";
system("basename", "/home/user/documents/");

# basename with current directory using backticks
print "\nbasename with current directory:\n";

print "Current directory: $basename_current";

# basename with parent directory using system()
print "\nbasename with parent directory:\n";
system("basename", "..");

# basename with root directory using backticks
print "\nbasename with root directory:\n";

print "Root directory: $basename_root";

# basename with empty string using system()
print "\nbasename with empty string:\n";
system("basename", "");

# basename with relative path using backticks
print "\nbasename with relative path:\n";

print "Relative path: $basename_relative";

# basename with hidden file using system()
print "\nbasename with hidden file:\n";
system("basename", "/path/to/.hidden.txt");

# basename with file without extension using backticks
print "\nbasename with file without extension:\n";

print "File without extension: $basename_no_ext";

# basename with multiple extensions using system()
print "\nbasename with multiple extensions:\n";
system("basename", "/path/to/file.txt.bak", ".txt.bak");

# Clean up
print "=== Example 027 completed successfully ===\n";
