#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $ls_success = 0;
our $CHILD_ERROR;

print "=== System Utilities ===\n";
my $formatted_date = do {
require POSIX; POSIX::strftime('%Y-%m-%d', localtime())
};
print "Formatted date: $formatted_date\n";
my $yes_result = do { open(my $__fh, '-|', 'bash', '-c', q{yes Hello | head -3}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print "Yes command result:\n";
print $yes_result, "\n";

