#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
our $CHILD_ERROR;
$ENV{ARGS} = (defined "@ARGV" && "@ARGV" ne q{} ? "@ARGV" : '');
print($ENV{ARGS}, "\n");
