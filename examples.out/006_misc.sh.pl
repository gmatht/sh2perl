#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '006_misc.sh';
say "== Subshell ==";
do {
    local %ENV = %ENV;
    say 'inside-subshell';
    q{};
};
say "== Simple pipeline ==";
# Original bash: echo "alpha beta" | grep beta
do {
    my $output_136 = q{};
    my $output_printed_136;
    my $pipeline_success_136 = 1;
    $output_136 .= 'alpha beta' . "\n";
if ( !($output_136 =~ m{\n\z}) ) { $output_136 .= "\n"; }

        my $grep_result_136_1;
    my @grep_lines_136_1 = split /\n/msx, $output_136;
    my @grep_filtered_136_1 = grep { /beta/msx } @grep_lines_136_1;
    $grep_result_136_1 = join "\n", @grep_filtered_136_1;
    if (!($grep_result_136_1 =~ m{\n\z} || $grep_result_136_1 eq q{})) {
    $grep_result_136_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_136_1 > 0 ? 0 : 1;
    $output_136 = $grep_result_136_1;
    $output_136 = $grep_result_136_1;
    if ((scalar @grep_filtered_136_1) == 0) {
        $pipeline_success_136 = 0;
    }
    if ($output_136 ne q{} && !defined $output_printed_136) {
        print $output_136;
        if (!($output_136 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_136 ) { $main_exit_code = 1; }
    }
