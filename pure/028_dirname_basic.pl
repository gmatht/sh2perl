#!/usr/bin/perl

# Example 028: Basic dirname command using system() and backticks
# This demonstrates the dirname builtin called from Perl

print "=== Example 028: Basic dirname command ===\n";

# Simple dirname using backticks
print "Using backticks to call dirname:\n";

print "dirname /path/to/file.txt: $dirname_output";

# dirname with multiple paths using system()
print "\ndirname with multiple paths:\n";
system("dirname", "/path/to/file1.txt", "/path/to/file2.txt", "/path/to/file3.txt");

# dirname with current directory using backticks
print "\ndirname with current directory:\n";

print "Current directory: $dirname_current";

# dirname with parent directory using system()
print "\ndirname with parent directory:\n";
system("dirname", "..");

# dirname with root directory using backticks
print "\ndirname with root directory:\n";

print "Root directory: $dirname_root";

# dirname with empty string using system()
print "\ndirname with empty string:\n";
system("dirname", "");

# dirname with relative path using backticks
print "\ndirname with relative path:\n";

print "Relative path: $dirname_relative";

# dirname with hidden file using system()
print "\ndirname with hidden file:\n";
system("dirname", "/path/to/.hidden.txt");

# dirname with file in root using backticks
print "\ndirname with file in root:\n";

print "File in root: $dirname_root_file";

# dirname with directory path using system()
print "\ndirname with directory path:\n";
system("dirname", "/home/user/documents/");

# dirname with nested path using backticks
print "\ndirname with nested path:\n";

print "Nested path: $dirname_nested";

# dirname with single level path using system()
print "\ndirname with single level path:\n";
system("dirname", "/file.txt");

# dirname with multiple levels using backticks
print "\ndirname with multiple levels:\n";

print "Multiple levels: $dirname_multi";

# dirname with zero option using system()
print "\ndirname with zero option (-z):\n";
system("dirname", "-z", "/path/to/file.txt");

# Clean up
print "=== Example 028 completed successfully ===\n";
