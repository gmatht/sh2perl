#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
my $ls_success = 0;
our $CHILD_ERROR;
$0 = '000__05_system_utilities.sh';
print "=== System Utilities ===\n";
my $formatted_date = do { my $__cs = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime())
}; chomp $__cs; $__cs; };
print "Formatted date: ${formatted_date}\n";
my $yes_result = do { open(my $__fh, '-|', 'bash', '-c', 'yes Hello | head -3') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print "Yes command result:\n";
print($yes_result, "\n");
