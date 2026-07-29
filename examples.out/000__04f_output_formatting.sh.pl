#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use File::Path qw(make_path remove_tree);
my $ls_success = 0;
our $CHILD_ERROR;

print "=== Output and Formatting Commands ===\n";
my $echo_result = "Hello from backticks";
print "Echo result: $echo_result\n";
my $printf_result = sprintf("Number: %d, String: %s\n", 42, "test");
print "Printf result: $printf_result\n";
my $tee_result = do { open(my $__fh, '-|', 'bash', '-c', q{echo 'test output' | tee test_tee.txt}) or die "cmd failed: $!\n"; local $/; my $_r = <$__fh>; close $__fh; $CHILD_ERROR = $? >> 8; $_r; };
print "Tee result: $tee_result\n";
unlink('test_tee.txt');
print "=== Output and Formatting Commands Complete ===\n";

