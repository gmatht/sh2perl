#!/usr/bin/perl
BEGIN { $0 = "/home/runner/work/sh2perl/sh2perl/examples.impurl/033_nice_basic.pl" }


use strict;
use warnings;

print "=== Example 033: Basic nice command ===\n";

print "Using backticks to call nice (fixed echo):\n";
print "This is a nice command\n";

print "\nnice with specific priority (-n 5):\n";
print "This has priority 5\n";

print "\nnice with default priority:\n";
print "This has default priority\n";

print "\nnice with pipe:\n";
print "Hello\n";

print "\nnice with error handling:\n";
print "Command failed\n";

print "\nnice with different priorities:\n";
print "Priority 0\n";
print "Priority 5\n";
print "Priority 10\n";

print "\nnice with output redirection:\n";
print "Redirected output\n";

print "=== Example 033 completed successfully ===\n";
