#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
print "Hello, World!\n";
if (-f "test.txt") {
    print "File exists\n";
}
for my $i (1..5) {
    print $i, "\n";
}

