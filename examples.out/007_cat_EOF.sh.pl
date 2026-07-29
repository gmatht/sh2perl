#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
$PROGRAM_NAME = '007_cat_EOF.sh';
print "alpha
beta
gamma ...
";
print "oyster
snapper
salmon
";
print "Fin. That is all folks.\n";
print "exit: ${\($? >> 8)}\n";
