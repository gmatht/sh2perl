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
my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '062_10_simple_pipeline.sh';
print "Testing simple pipeline...\n";
{
    my $output_285 = q{};
    my $output_printed_285;
    my $pipeline_success_285 = 1;
        $output_285 = do { my $command = 'ls -la'; my $result = qx{$command}; $CHILD_ERROR = $? >> 8; $result; };

        my $grep_result_285_1;
    my @grep_lines_285_1 = split /\n/msx, $output_285;
    my @grep_filtered_285_1 = grep { /^d/msx } @grep_lines_285_1;
    $grep_result_285_1 = join "\n", @grep_filtered_285_1;
    if (!($grep_result_285_1 =~ m{\n\z}msx || $grep_result_285_1 eq q{})) {
    $grep_result_285_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_285_1 > 0 ? 0 : 1;
    $output_285 = $grep_result_285_1;
    $output_285 = $grep_result_285_1;

        my $num_lines       = 5;
    my $head_line_count = 0;
    my $result          = q{};
    my $input           = $output_285;
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
    $output_285 = $result;
    if ($output_285 ne q{} && !defined $output_printed_285) {
        print $output_285;
        if (!($output_285 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_285 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;
