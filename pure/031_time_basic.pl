#!/usr/bin/perl

# Example 031: Basic time command using system() and backticks
# This demonstrates the time builtin called from Perl

print "=== Example 031: Basic time command ===\n";

# Simple time using backticks
print "Using backticks to call time (time ls):\n";

print $time_output;

# time with specific command using system()
print "\ntime with specific command (time sleep 1):\n";
system("time", "sleep", "1");

# time with verbose using backticks
print "\ntime with verbose (-v):\n";

print $time_verbose;

# time with format using system()
print "\ntime with format (-f '%E'):\n";
system("time", "-f", "%E", "sleep", "1");

# time with multiple format specifiers using backticks
print "\ntime with multiple format specifiers:\n";

print $time_multi;

# time with custom format using system()
print "\ntime with custom format:\n";
system("time", "-f", "Total time: %E seconds", "ls", "-la");

# time with pipe using backticks
print "\ntime with pipe:\n";

print $time_pipe;

# time with multiple commands using system()
print "\ntime with multiple commands:\n";
system("time", "ls", "&&", "echo", "Command completed");

# time with background process using backticks
print "\ntime with background process:\n";

print $time_bg;

# time with error handling using system()
print "\ntime with error handling:\n";
system("time", "nonexistent_command", "2>/dev/null", "||", "echo", "Command failed");

# time with output redirection using backticks
print "\ntime with output redirection:\n";

print "Time output: $time_redirect";

# time with different time formats using system()
print "\ntime with different time formats:\n";
system("time", "-f", "Real: %E", "sleep", "1");
system("time", "-f", "User: %U", "sleep", "1");
system("time", "-f", "System: %S", "sleep", "1");

# time with cumulative time using backticks
print "\ntime with cumulative time:\n";
my $time_cumulative = `time -f 'Cumulative: %E' (sleep 1; sleep 1) 2>&1`;
print $time_cumulative;

print "=== Example 031 completed successfully ===\n";
