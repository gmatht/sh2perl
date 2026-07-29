#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
print "Testing ambiguous operators...\n";
my $result = int(2**3**2);
print "2**3**2 = $result\n";

