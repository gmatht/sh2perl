#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '095_select_menu.sh';
# Original bash: echo "select" | head -1
do {
    my $output_394 = q{};
    my $output_printed_394;
    my $pipeline_success_394 = 1;
    $output_394 .= 'select' . "\n";
if ( !($output_394 =~ m{\n\z}) ) { $output_394 .= "\n"; }

        my $num_lines       = 1;
    my $head_line_count = 0;
    my $result          = q{};
    my $input           = $output_394;
    my $pos             = 0;
    while ( $pos < length $input && $head_line_count < $num_lines ) {
    my $line_end = index $input, "\n", $pos;
    if ( $line_end == -1 ) {
    $line_end = length $input;
    }
    my $head_line = substr $input, $pos, $line_end - $pos;
    $result .= $head_line . "\n";
    $pos = $line_end + 1;
    ++$head_line_count;
    }
    $output_394 = $result;
    if ($output_394 ne q{} && !defined $output_printed_394) {
        print $output_394;
        if (!($output_394 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_394 ) { $main_exit_code = 1; }
    }
