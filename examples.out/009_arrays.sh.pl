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
    my $output_154 = q{};
    my $output_printed_154;
    my $pipeline_success_154 = 1;
        $output_154 = q{};
    my @output_154_items = (keys %map);
    for my $k (@output_154_items) {
    $output_154 .= "$k => " . $map{$k}. "\n";
    }

        my @sort_lines_154_1 = split /\n/msx, $output_154;
    my @sort_sorted_154_1 = sort @sort_lines_154_1;
    my $output_154_1 = join "\n", @sort_sorted_154_1;
    if ($output_154_1 ne q{} && !($output_154_1 =~ m{\n\z}msx)) {
    $output_154_1 .= "\n";
    }
    $output_154 = $output_154_1;
    $output_154 = $output_154_1;
    if ($output_154 ne q{} && !defined $output_printed_154) {
        print $output_154;
        if (!($output_154 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_154 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }

exit $main_exit_code;
