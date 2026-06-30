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
my @arr;
@arr = ("one", "two", "three");
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
my @map_key_order = ();
$map{"foo"} = 'bar';
push @map_key_order, "foo";
$map{"answer"} = '42';
push @map_key_order, "answer";
$map{"two"} = "1 + 1";
push @map_key_order, "two";
print $map{foo};
if ( !( $map{foo} =~ m{\n\z}msx ) ) { print "\n"; }
print $map{answer};
if ( !( $map{answer} =~ m{\n\z}msx ) ) { print "\n"; }
{
    my $output_156 = q{};
    my $output_printed_156;
    my $pipeline_success_156 = 1;
        $output_156 = q{};
    my @output_156_items = (keys %map);
    for my $k (@output_156_items) {
    $output_156 .= "$k => " . $map{$k}. "\n";
    }

        my @sort_lines_156_1 = split /\n/msx, $output_156;
    my @sort_sorted_156_1 = sort @sort_lines_156_1;
    my $output_156_1 = join "\n", @sort_sorted_156_1;
    if ($output_156_1 ne q{} && !($output_156_1 =~ m{\n\z}msx)) {
    $output_156_1 .= "\n";
    }
    $output_156 = $output_156_1;
    $output_156 = $output_156_1;
    if ($output_156 ne q{} && !defined $output_printed_156) {
        print $output_156;
        if (!($output_156 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_156 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;
