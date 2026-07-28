#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '000__05_system_utilities.sh';
say "=== System Utilities ===";
my $formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime())
};
say "Formatted date: $formatted_date";
my $yes_result = do { chomp(my $result_112 = qx{yes Hello | head -3}); $result_112; };
say "Yes command result:";
say $yes_result;
