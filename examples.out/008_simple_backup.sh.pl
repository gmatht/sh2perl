#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
my $ls_success = 0;
my $output = '';
our $CHILD_ERROR;
$0 = '008_simple_backup.sh';
print "Hello, World!\n";
# Original bash: ls -1 | grep -v __tmp_test_output.pl
my $output_133 = do { open(my $__fh, '-|', 'bash', '-c', 'ls -1 | grep -v __tmp_test_output.pl') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; };
print($output_133, "\n");
print(join(" ", grep { length } split /\s+/msx, do { open(my $__fh, '-|', 'bash', '-c', 'ls | grep -v __tmp_test_output.pl') or die "cmd failed: $!\n"; my $_r = do { local $/; <$__fh> }; close $__fh; chomp $_r; $CHILD_ERROR = $? >> 8; $_r; }), "\n");

exit $main_exit_code;
