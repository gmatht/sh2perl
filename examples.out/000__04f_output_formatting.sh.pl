#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
use File::Path qw(make_path remove_tree);
our $CHILD_ERROR;

$PROGRAM_NAME = '000__04f_output_formatting.sh';
print "=== Output and Formatting Commands ===\n";
my $echo_result = "Hello from backticks";
print "Echo result: $echo_result\n";
my $printf_result = sprintf("Number: %d, String: %s\n", 42, "test");
print "Printf result: $printf_result\n";
my $tee_result = do { chomp(my $_r = qx{command echo 'test output' | tee test_tee.txt}); $_r; };
print "Tee result: $tee_result\n";
unlink('test_tee.txt');
print "=== Output and Formatting Commands Complete ===\n";
