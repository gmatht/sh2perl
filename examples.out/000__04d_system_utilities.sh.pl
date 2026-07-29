#!/usr/bin/env perl
use strict;
use warnings;
use IPC::Open3;
our $CHILD_ERROR;

print "=== System Utilities ===\n";
my $formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime())
};
print "Formatted date: $formatted_date\n";
my $sleep_duration = "1";
print "Sleeping for $sleep_duration seconds...\n";
require Time::HiRes; Time::HiRes::sleep($sleep_duration);
my $yes_result = do { chomp(my $_r = qx{command yes Hello | head -3}); $_r; };
print "Yes command result:\n";
print $yes_result, "\n";
print "=== System Utilities Complete ===\n";

