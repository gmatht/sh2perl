#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
if (-f "file.txt") {
    print "File exists\n";
}
else {
    print "File does not exist\n";
}
print "exit: " . ($? >> 8), "\n";

