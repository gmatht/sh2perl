#!/usr/bin/env perl
use strict;
use warnings;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
my $main_exit_code = 0;
my $output = '';
our $CHILD_ERROR;

say "Multi-stage:";
# Original bash: echo "hello world" | tr ' ' '\n' | sort | head -2
my $output_0 = q{};
say $output_0;
say "done";

exit $main_exit_code;


