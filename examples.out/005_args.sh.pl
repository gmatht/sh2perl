#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
print "== Argument count ==\n";
print scalar(@ARGV), "\n";
print "== Arguments ==\n";
for my $a (@ARGV) {
    print "Arg: $a\n";
}

