#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use IPC::Open3;
my $main_exit_code = 0;
my $output = '';
our $CHILD_ERROR;
if (!("${VAR}" ne q{})) {
        my $VAR = "default";
}

sub my_func {
    print "hello\n";
    return;
}
my_func();

exit $main_exit_code;
