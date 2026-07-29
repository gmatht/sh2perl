#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
print "alpha\nbeta\ngamma ...\n";
print "oyster\nsnapper\nsalmon\n";
print "Fin. That is all folks.\n";
print "exit: " . ($? >> 8), "\n";

