#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
my $__set_e        = 0;
our $CHILD_ERROR;

my $output_274 = q{};
my $output_printed_274;
my $head_line_count = 0;
my $output_275 = q{};
while (my $line = <>) {
    chomp $line;
    # find doesn't support line-by-line processing
    if ($head_line_count < 3) {
    $output_275 .= $line . "\n";
    ++$head_line_count;
} else {
    $line = q{}; # Clear line to prevent printing
    last; # Break out of the yes loop when head limit is reached
}
    print $line . "\n";
}

exit $main_exit_code;
