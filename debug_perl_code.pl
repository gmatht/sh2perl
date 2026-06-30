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
    my $output_0 = q{};
    my $output_printed_0;
    my $pipeline_success_0 = 1;
        $output_0 = q{};
    my @output_0_items = (keys %map);
    for my $k (@output_0_items) {
    $output_0 .= "$k => " . $map{$k}. "\n";
    }

        my @sort_lines_0_1 = split /\n/msx, $output_0;
    my @sort_sorted_0_1 = sort @sort_lines_0_1;
    my $output_0_1 = join "\n", @sort_sorted_0_1;
    if ($output_0_1 ne q{} && !($output_0_1 =~ m{\n\z}msx)) {
    $output_0_1 .= "\n";
    }
    $output_0 = $output_0_1;
    $output_0 = $output_0_1;
    if ($output_0 ne q{} && !defined $output_printed_0) {
        print $output_0;
        if (!($output_0 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_0 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;
