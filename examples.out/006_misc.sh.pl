#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

my $perl_output_0 = do {
            my $result = qx{perl };
            chomp $result;
            $result;
        };
print $perl_output_0;

exit $main_exit_code;


Exit code: exit status: 2

