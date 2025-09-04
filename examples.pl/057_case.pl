#!/usr/bin/env perl

use strict;
use warnings;

# Case statement examples
# This demonstrates Perl switch statement syntax and common usage patterns

print "=== Basic Case Statement Example ===\n";

# Example 1: Basic case statement with simple patterns
my $action = $ARGV[0] // "";
if ($action eq "start") {
    print "Starting the service...\n";
} elsif ($action eq "stop") {
    print "Stopping the service...\n";
} elsif ($action eq "restart") {
    print "Restarting the service...\n";
} else {
    print "Usage: $0 {start|stop|restart}\n";
    exit 1;
}

print "=== Case Statement with Pattern Matching ===\n";

# Example 2: Case statement with pattern matching
my $filename = $ARGV[1] // "";
if ($filename =~ /\.txt$/) {
    print "Processing text file: $filename\n";
} elsif ($filename =~ /\.sh$/) {
    print "Processing shell script: $filename\n";
} elsif ($filename =~ /\.py$/) {
    print "Processing Python file: $filename\n";
} else {
    print "Unknown file type: $filename\n";
}

print "=== Case Statement with Multiple Patterns ===\n";

# Example 3: Case statement with multiple patterns per case
my $option = $ARGV[2] // "";
if ($option =~ /^(help|h|-h|--help)$/) {
    print "Help information:\n";
    print "  start  - Start the service\n";
    print "  stop   - Stop the service\n";
    print "  status - Show service status\n";
} elsif ($option =~ /^(status|s|-s|--status)$/) {
    print "Service status: Running\n";
} else {
    print "Unknown option: $option\n";
}

print "=== Case Statement with Character Classes ===\n";

# Example 4: Case statement with character classes
my $char = $ARGV[3] // "";
if ($char =~ /^[0-9]$/) {
    print "Single digit: $char\n";
} elsif ($char =~ /^[a-z]$/) {
    print "Lowercase letter: $char\n";
} elsif ($char =~ /^[A-Z]$/) {
    print "Uppercase letter: $char\n";
} elsif ($char =~ /^[0-9][0-9]$/) {
    print "Two digit number: $char\n";
} else {
    print "Other character: $char\n";
}

print "=== Case Statement with Default Action ===\n";

# Example 5: Case statement with default action
my $color = $ARGV[4] // "";
if ($color eq "red") {
    print "Color is red\n";
} elsif ($color eq "green") {
    print "Color is green\n";
} elsif ($color eq "blue") {
    print "Color is blue\n";
}

print "=== Case Statement with Commands ===\n";

# Example 6: Case statement with command execution
my $command = $ARGV[5] // "";
if ($command eq "ls") {
    system("ls -la");
} elsif ($command eq "date") {
    system("date");
} elsif ($command eq "pwd") {
    system("pwd");
} elsif ($command eq "whoami") {
    system("whoami");
} else {
    print "Available commands: ls, date, pwd, whoami\n";
}

