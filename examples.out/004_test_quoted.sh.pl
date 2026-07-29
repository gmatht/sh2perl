#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
print "Hello, World!\n";
print "Single quoted\
";
print "String with \"escaped\" quotes\n";
print "String with 'single' quotes\n";
print "exit: " . ($? >> 8), "\n";

