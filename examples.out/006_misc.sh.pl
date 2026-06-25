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

$PROGRAM_NAME = '006_misc.sh';
print "== Subshell ==\n";
do {
    local %ENV = %ENV;
    print 'inside-subshell' . "\n";
    $CHILD_ERROR = 0;
    q{};
};
print "== Simple pipeline ==\n";
{
    my $output_46 = q{};
    my $output_printed_46;
    my $pipeline_success_46 = 1;
    $output_46 .= 'alpha beta' . "\n";
if ( !($output_46 =~ m{\n\z}msx) ) { $output_46 .= "\n"; }
$CHILD_ERROR = 0;

        my $grep_result_46_1;
    my @grep_lines_46_1 = split /\n/msx, $output_46;
    my @grep_filtered_46_1 = grep { /beta/msx } @grep_lines_46_1;
    $grep_result_46_1 = join "\n", @grep_filtered_46_1;
    if (!($grep_result_46_1 =~ m{\n\z}msx || $grep_result_46_1 eq q{})) {
    $grep_result_46_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_46_1 > 0 ? 0 : 1;
    $output_46 = $grep_result_46_1;
    $output_46 = $grep_result_46_1;
    if ((scalar @grep_filtered_46_1) == 0) {
        $pipeline_success_46 = 0;
    }
    if ($output_46 ne q{} && !defined $output_printed_46) {
        print $output_46;
        if (!($output_46 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_46 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;
