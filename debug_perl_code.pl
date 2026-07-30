#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $output = '';
our $CHILD_ERROR;
$0 = 'parse-dollar-single-quote.sh';
$ENV{IFS} = "\n\t";
printf('IFS value (hex)=');
printf('%q', "$ENV{IFS}");
printf("\n");
