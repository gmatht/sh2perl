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

$PROGRAM_NAME = '009_arrays.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
print "== Indexed arrays ==\n";
my @arr = ("one", "two", "three");
print $arr[1];
if ( !( ($arr[1]) =~ m{\n\z}msx ) ) { print "\n"; }
print scalar(@arr) . "\n";
$CHILD_ERROR = 0;
my $x;
for my $x (@arr) {
printf('%s ', "$x");
}
print "\n";
$CHILD_ERROR = 0;
print "== Associative arrays ==\n";
my %map = ();
# declare map not implemented
$map{"foo"} = 'bar';
$map{"two"} = "1 + 1";
$map{"answer"} = '42';
print $map{foo};
if ( !( ($map{foo}) =~ m{\n\z}msx ) ) { print "\n"; }
print $map{answer};
if ( !( ($map{answer}) =~ m{\n\z}msx ) ) { print "\n"; }
{
    my $output_148 = q{};
    my $output_printed_148;
    my $pipeline_success_148 = 1;
        $output_148 = q{};
    my @output_148_items = (keys %map);
    for my $k (@output_148_items) {
    $output_148 .= "$k => " . $map{$k}. "\n";
    }

        my @sort_lines_148_1 = split /\n/msx, $output_148;
    my @sort_sorted_148_1 = sort @sort_lines_148_1;
    my $output_148_1 = join "\n", @sort_sorted_148_1;
    if ($output_148_1 ne q{} && !($output_148_1 =~ m{\n\z}msx)) {
    $output_148_1 .= "\n";
    }
    $output_148 = $output_148_1;
    $output_148 = $output_148_1;
    if ($output_148 ne q{} && !defined $output_printed_148) {
        print $output_148;
        if (!($output_148 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_148 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }

exit $main_exit_code;
