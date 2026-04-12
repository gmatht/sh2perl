#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars);
use locale;
select((select(STDOUT), $| = 1)[0]);
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success     = 0;
our $CHILD_ERROR;

print "== Subshell ==\n";
do {
    print 'inside-subshell' . "\n";
};
print "== Simple pipeline ==\n";
{
    my $output_156;
    my $output_printed_156;
    my $pipeline_success_156 = 1;
    $output_156 .= "alpha beta\n";
if ( !($output_156 =~ m{\n\z}msx) ) { $output_156 .= "\n"; }

        my $grep_result_156_1;
    my @grep_lines_156_1 = split /\n/msx, $output_156;
    my @grep_filtered_156_1 = grep { /beta/msx } @grep_lines_156_1;
    $grep_result_156_1 = join "\n", @grep_filtered_156_1;
    if (!($grep_result_156_1 =~ m{\n\z}msx || $grep_result_156_1 eq q{})) {
    $grep_result_156_1 .= "\n";
    }
    $CHILD_ERROR = scalar @grep_filtered_156_1 > 0 ? 0 : 1;
    $output_156 = $grep_result_156_1;
    $output_156 = $grep_result_156_1;
    if ((scalar @grep_filtered_156_1) == 0) {
        $pipeline_success_156 = 0;
    }
    if ($output_156 ne q{} && !defined $output_printed_156) {
        print $output_156;
        if (!($output_156 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_156 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;
