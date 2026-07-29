#!/usr/bin/env perl
use strict;
use warnings;
use IPC::Open3;
our $CHILD_ERROR;

$PROGRAM_NAME = '000__05_system_utilities.sh';
print "=== System Utilities ===\n";
my $formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime())
};
print "Formatted date: $formatted_date\n";
my $yes_result = do { chomp(my $_r = qx{command yes Hello | head -3}); $_r; };
print "Yes command result:\n";
print $yes_result, "\n";
