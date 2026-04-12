#!/usr/bin/perl

# Example 032: Basic kill command using system() and backticks
# This demonstrates the kill builtin called from Perl

print "=== Example 032: Basic kill command ===\n";

# Get current process ID
my $pid = $$;
print "Current process ID: $pid\n";

# Simple kill with signal using backticks
print "Using backticks to call kill (kill -0 to check if process exists):\n";

if ($? == 0) {
    print "Process $pid exists\n";
} else {
    print "Process $pid does not exist\n";
}

# kill with specific signal using system()
print "\nkill with specific signal (SIGTERM):\n";
system("kill", "-TERM", $pid);

# kill with signal number using backticks
print "\nkill with signal number (15 for SIGTERM):\n";

print "Kill signal result: $kill_signal";

# kill with list signals using system()
print "\nkill with list signals (-l):\n";
system("kill", "-l");

# kill with all processes using backticks
print "\nkill with all processes (kill -0):\n";

if ($? == 0) {
    print "Process 1 (init) exists\n";
} else {
    print "Process 1 (init) does not exist\n";
}

# kill with process group using system()
print "\nkill with process group:\n";
system("kill", "-0", "$$");

# kill with user processes using backticks
print "\nkill with user processes:\n";
my $kill_user = `kill -0 $$ 2>&1`;
print "User process result: $kill_user";

# kill with specific signal names using system()
print "\nkill with specific signal names:\n";
system("kill", "-HUP", $pid);
system("kill", "-INT", $pid);
system("kill", "-QUIT", $pid);

# kill with error handling using backticks
print "\nkill with error handling (kill non-existent process):\n";

print "Error result: $kill_error";

# kill with multiple PIDs using system()
print "\nkill with multiple PIDs:\n";
system("kill", "-0", "$$", "1");

# kill with signal names using backticks
print "\nkill with signal names:\n";
my $kill_names = `kill -0 $$ 2>&1`;
print "Signal names result: $kill_names";

# kill with timeout using system()
print "\nkill with timeout:\n";
system("timeout", "1", "kill", "-0", "$$");

print "=== Example 032 completed successfully ===\n";
