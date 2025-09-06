#!/usr/bin/perl

# Example 033: Basic nice command using system() and backticks
# This demonstrates the nice builtin called from Perl

print "=== Example 033: Basic nice command ===\n";

# Simple nice using backticks
print "Using backticks to call nice (nice echo):\n";
my $nice_output = `nice echo "This is a nice command"`;
print $nice_output;

# nice with specific priority using system()
print "\nnice with specific priority (-n 5):\n";
system("nice", "-n", "5", "echo", "This has priority 5");

# nice with high priority using backticks
print "\nnice with high priority (-n -5):\n";
my $nice_high = `nice -n -5 echo "This has high priority"`;
print $nice_high;

# nice with low priority using system()
print "\nnice with low priority (-n 10):\n";
system("nice", "-n", "10", "echo", "This has low priority");

# nice with default priority using backticks
print "\nnice with default priority:\n";
my $nice_default = `nice echo "This has default priority"`;
print $nice_default;

# nice with sleep command using system()
print "\nnice with sleep command:\n";
system("nice", "-n", "5", "sleep", "1");

# nice with multiple commands using backticks
print "\nnice with multiple commands:\n";
my $nice_multi = `nice -n 3 (echo "Command 1"; echo "Command 2")`;
print $nice_multi;

# nice with background process using system()
print "\nnice with background process:\n";
system("nice", "-n", "5", "sleep", "2", "&");

# nice with pipe using backticks
print "\nnice with pipe:\n";
my $nice_pipe = `nice -n 3 echo "Hello" | cat`;
print $nice_pipe;

# nice with error handling using system()
print "\nnice with error handling:\n";
system("nice", "-n", "5", "nonexistent_command", "2>/dev/null", "||", "echo", "Command failed");

# nice with different priorities using backticks
print "\nnice with different priorities:\n";
my $nice_0 = `nice -n 0 echo "Priority 0"`;
my $nice_5 = `nice -n 5 echo "Priority 5"`;
my $nice_10 = `nice -n 10 echo "Priority 10"`;
print $nice_0;
print $nice_5;
print $nice_10;

# nice with output redirection using system()
print "\nnice with output redirection:\n";
system("nice", "-n", "5", "echo", "Redirected output", ">", "/dev/null");

# nice with environment variables using backticks
print "\nnice with environment variables:\n";
my $nice_env = `nice -n 3 env | head -3`;
print $nice_env;

print "=== Example 033 completed successfully ===\n";
