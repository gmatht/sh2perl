#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $main_exit_code = 0;
my $ls_success = 0;
my $output = '';
our $CHILD_ERROR;

print "Hello, World!\n";
# Original bash: ls -1 | grep -v __tmp_test_output.pl
my $output_0 = do { open(my $__fh, '-|', 'bash', '-c', q{ls -1 | grep -v __tmp_test_output.pl}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print $output_0, "\n";
print join(" ", grep { length } split /\s+/msx, do { open(my $__fh, '-|', 'bash', '-c', q{ls | grep -v __tmp_test_output.pl}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; });

exit $main_exit_code;

