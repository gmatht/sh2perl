#!/usr/bin/perl

# Example 044: yes command using system() and backticks
# This demonstrates the yes builtin called from Perl

print "=== Example 044: yes command ===\n";

# Simple yes command using backticks
print "Using backticks to call yes (limited output):\n";

print $yes_output;

# yes with specific string using system()
print "\nyes with specific string:\n";
system("yes", "Test String", "|", "head", "-3");

# yes with default string using backticks
print "\nyes with default string:\n";

print $yes_default;

# yes with empty string using system()
print "\nyes with empty string:\n";
system("yes", "", "|", "head", "-3");

# yes with special characters using backticks
print "\nyes with special characters:\n";
my $yes_special = `yes "!@#$%^&*()" | head -3`;
print $yes_special;

# yes with numbers using system()
print "\nyes with numbers:\n";
system("yes", "12345", "|", "head", "-3");

# yes with newlines using backticks
print "\nyes with newlines:\n";

print $yes_newlines;

# yes with pipe to other commands using system()
print "\nyes with pipe to other commands:\n";
system("yes", "test", "|", "grep", "test", "|", "head", "-3");

# yes with pipe to other commands using backticks
print "\nyes with pipe to other commands:\n";

print $yes_pipe;

# yes with output redirection using system()
print "\nyes with output redirection:\n";
system("yes", "Output to file", "|", "head", "-5", ">", "yes_output.txt");

# Check if output file was created
if (-f "yes_output.txt") {
    print "Output file created successfully\n";
    
    print "File content:\n$file_content";
}

# yes with background process using backticks
print "\nyes with background process:\n";

print "Background process started\n";

# yes with timeout using system()
print "\nyes with timeout:\n";
system("timeout", "1", "yes", "Timeout test");

# yes with different strings using backticks
print "\nyes with different strings:\n";

print $yes_diff;

print $yes_diff2;

# yes with error handling using system()
print "\nyes with error handling:\n";
system("yes", "Error test", "2>/dev/null", "|", "head", "-3");

# yes with pipe to wc using backticks
print "\nyes with pipe to wc:\n";

print "Count: $yes_wc";

# Clean up
unlink('yes_output.txt') if -f 'yes_output.txt';

print "=== Example 044 completed successfully ===\n";
