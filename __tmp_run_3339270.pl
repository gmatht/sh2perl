#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
my $output = '';
our $CHILD_ERROR;

my $myvar;

if (!("${myvar}" ne q{})) {
        $myvar = "default";
}
print($myvar, "\n");

exit $main_exit_code;
