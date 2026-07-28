#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '000__04d_system_utilities.sh';
say "=== System Utilities ===";
my $formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime())
};
say "Formatted date: $formatted_date";
my $sleep_duration = "1";
say "Sleeping for $sleep_duration seconds...";
require Time::HiRes; Time::HiRes::sleep($sleep_duration);
my $yes_result = do { chomp(my $result_62 = qx{yes Hello | head -3}); $result_62; };
say "Yes command result:";
say $yes_result;
say "=== System Utilities Complete ===";
