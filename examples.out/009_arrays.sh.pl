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

$SIG{__DIE__} = sub { exit 1 };
# set uo not implemented
# set pipefail not implemented
print "== Indexed arrays ==\n";
my @arr = ("one", "two", "three");
print $arr[1];
if ( !( $arr[1] =~ m{\n\z}msx ) ) { print "\n"; }
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
$map{"answer"} = '42';
$map{"two"} = "1 + 1";
print $map{foo};
if ( !( $map{foo} =~ m{\n\z}msx ) ) { print "\n"; }
print $map{answer};
if ( !( $map{answer} =~ m{\n\z}msx ) ) { print "\n"; }
{
    my $output_1 = q{};
    my $output_printed_1;
    my $pipeline_success_1 = 1;
        $output_1 = q{};
    my @output_1_items = (keys %map);
    for my $k (@output_1_items) {
    $output_1 .= "$k => " . $map{$k}. "\n";
    }

        my @sort_lines_1_1 = split /\n/msx, $output_1;
    my @sort_sorted_1_1 = sort @sort_lines_1_1;
    my $output_1_1 = join "\n", @sort_sorted_1_1;
    if ($output_1_1 ne q{} && !($output_1_1 =~ m{\n\z}msx)) {
    $output_1_1 .= "\n";
    }
    $output_1 = $output_1_1;
    $output_1 = $output_1_1;
    if ($output_1 ne q{} && !defined $output_printed_1) {
        print $output_1;
        if (!($output_1 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_1 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;
