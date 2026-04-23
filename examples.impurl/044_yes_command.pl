#!/usr/bin/perl

# Example 044: yes command using system() and backticks
# This demonstrates the yes builtin called from Perl

print "=== Example 044: yes command ===\n";

# Simple yes command using backticks
print "Using backticks to call yes (limited output):\n";
my $yes_output = `yes "Hello World" | head -5`;
print $yes_output;

# yes with specific string using system()
print "\nyes with specific string:\n";
system("sh", "-c", "yes 'Test String' | head -3");

# yes with default string using backticks
print "\nyes with default string:\n";
my $yes_default = `yes | head -3`;
print $yes_default;

# yes with empty string using system()
print "\nyes with empty string:\n";
system("sh", "-c", "yes '' | head -3");

# yes with special characters using backticks
print "\nyes with special characters:\n";
# Escape the dollar sign so Perl does not interpolate it inside the backtick
# (the intent is to pass a literal "$" to the shell's yes command).
my $yes_special = `yes "!@#\$%^&*()" | head -3`;
print $yes_special;

# yes with numbers using system()
print "\nyes with numbers:\n";
system("sh", "-c", "yes 12345 | head -3");

# yes with newlines using backticks
print "\nyes with newlines:\n";
my $yes_newlines = `yes "Line with\\nnewline" | head -3`;
print $yes_newlines;

# yes with pipe to other commands using system()
print "\nyes with pipe to other commands:\n";
system("sh", "-c", "yes test | grep test | head -3");

# yes with pipe to other commands using backticks
print "\nyes with pipe to other commands:\n";
my $yes_pipe = `yes "data" | tr 'a-z' 'A-Z' | head -3`;
print $yes_pipe;

# yes with output redirection using system()
print "\nyes with output redirection:\n";
system("sh", "-c", "yes 'Output to file' | head -5 > yes_output.txt");

# Check if output file was created
if (-f "yes_output.txt") {
    print "Output file created successfully\n";
    my $file_content = `cat yes_output.txt`;
    print "File content:\n$file_content";
}

# yes with background process (bounded) using system()
print "\nyes with background process (bounded):\n";
# spawn a short-lived background producer that exits after a few lines
system("sh", "-c", "yes 'Background' | head -n 3 > /dev/null &");
print "Background process started (will exit shortly)\n";

# yes with timeout (bounded) using head to limit output
print "\nyes with timeout (bounded):\n";
system("sh", "-c", "yes 'Timeout test' | head -n 3");

# yes with different strings using backticks
print "\nyes with different strings:\n";
my $yes_diff = `yes "String 1" | head -2`;
print $yes_diff;
my $yes_diff2 = `yes "String 2" | head -2`;
print $yes_diff2;

# yes with error handling using system()
print "\nyes with error handling:\n";
system("sh", "-c", "yes 'Error test' 2>/dev/null | head -3");

# yes with pipe to wc using backticks
print "\nyes with pipe to wc:\n";
my $yes_wc = `yes "Count me" | head -10 | wc -l`;
print "Count: $yes_wc";

# Clean up
unlink('yes_output.txt') if -f 'yes_output.txt';

print "=== Example 044 completed successfully ===\n";
