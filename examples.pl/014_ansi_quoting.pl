#!/usr/bin/env perl

use strict;
use warnings;

# ANSI-C quoting and special character examples
# Demonstrates escape sequences and special character handling

print "== ANSI-C quoting ==\n";
print "line1\nline2\tTabbed\n";

print "== Escape sequences ==\n";
print "bell\a\n";
print "backspace\b\n";
print "formfeed\f\n";
print "newline\n";
print "carriage\rreturn\n";
print "tab\tseparated\n";
print "vertical\vtab\n";

print "== Unicode and hex ==\n";
print "\x48\x65\x6c\x6c\x6f\n";  # Hello
print "\x48\x65\x6c\x6c\x6f\n";  # Hello

print "== Practical examples ==\n";
# Create a formatted table
printf "%-10s %-10s %s\n", "Name", "Age", "City";
printf "%-10s %-10s %s\n", "John", "25", "NYC";
printf "%-10s %-10s %s\n", "Jane", "30", "LA";
