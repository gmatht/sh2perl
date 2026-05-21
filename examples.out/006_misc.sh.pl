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

print "== Subshell ==\n";
do {
    print 'inside-subshell' . "\n";
    $CHILD_ERROR = 0;
    q{};
};
print "== Simple pipeline ==\n";
{
    my $output_149 = q{};
    my $output_printed_149;
    my $pipeline_success_149 = 1;
    $output_149 .= 'alpha beta' . "\n";
if ( !($output_149 =~ m{\n\z}msx) ) { $output_149 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_149_1;
    my @grep_lines_149_1 = split /\n/msx, $output_149;
    my @grep_filtered_149_1 = grep { /beta/msx } @grep_lines_149_1;
    $grep_result_149_1 = join "\n", @grep_filtered_149_1;
    if (!($grep_result_149_1 =~ m{\n\z}msx || $grep_result_149_1 eq q{})) {
    $grep_result_149_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_149_1 > 0 ? 0 : 1;
    $output_149 = $grep_result_149_1;
    $output_149 = $grep_result_149_1;
    if ((scalar @grep_filtered_149_1) == 0) {
        $pipeline_success_149 = 0;
    }
    if ($output_149 ne q{} && !defined $output_printed_149) {
        print $output_149;
        if (!($output_149 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_149 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;
