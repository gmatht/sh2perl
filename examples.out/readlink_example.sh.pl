#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
my $ls_success = 0;
our $CHILD_ERROR;

my $target = do { use Cwd qw(abs_path); my $_r = abs_path('/usr/bin/vi'); defined $_r ? $_r : q{}; };
print "vi resolves to: $target\n";
my $target2 = do { use Cwd qw(abs_path); my $_r = abs_path('/usr/bin/python3'); defined $_r ? $_r : q{}; };
print "python3 resolves to: $target2\n";

