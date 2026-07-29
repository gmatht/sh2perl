#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $ls_success = 0;
our $CHILD_ERROR;

my $relative = do { use Cwd qw(abs_path); my $_r = abs_path('/usr/bin/corepack'); defined $_r ? $_r : q{}; };
print "Corepack resolves to: $relative\n";

