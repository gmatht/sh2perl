#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
our $CHILD_ERROR;
$0 = '062_01_ambiguous_operators.sh';
print "Testing ambiguous operators...\n";
my $result = int(2**3**2);
print "2**3**2 = ${result}\n";
