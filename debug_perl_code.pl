#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
our $CHILD_ERROR;
$0 = '064_06_nested_arithmetic_expressions.sh';
my $i = eval { int(1 + (2 * 3) / 4) } // "";
my $j = int($i++ + ++$i);
print "i=$i, j=$j\n";
