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

$SIG{__DIE__} = sub { exit 1 };
# set uo not implemented
# set pipefail not implemented
print "== Indexed arrays ==\n";
my @arr = ("one", "two", "three");
print $arr[1];
if ( !( $arr[1] =~ m{\n\z}msx ) ) { print "\n"; }
print scalar(@arr) . "\n";
my $x;
for my $x (@arr) {
printf("%s ", "$x");
}
print "\n";
print "== Associative arrays ==\n";
my %map = ();
# declare map not implemented
$map{"foo"} = 'bar';
$map{"two"} = "1 + 1";
$map{"answer"} = '42';
print $map{foo};
if ( !( $map{foo} =~ m{\n\z}msx ) ) { print "\n"; }
print $map{answer};
if ( !( $map{answer} =~ m{\n\z}msx ) ) { print "\n"; }
{
    my $output_164;
    my $output_printed_164;
    my $pipeline_success_164 = 1;
        $output_164 = q{};
    my @output_164_items = (keys %map);
    for my $k (@output_164_items) {
    $output_164 .= "$k => " . $map{$k}. "\n";
    }

        my @sort_lines_164_1 = split /\n/msx, $output_164;
    my @sort_sorted_164_1 = sort @sort_lines_164_1;
    my $output_164_1 = join "\n", @sort_sorted_164_1;
    if ($output_164_1 ne q{} && !($output_164_1 =~ m{\n\z}msx)) {
    $output_164_1 .= "\n";
    }
    $output_164 = $output_164_1;
    $output_164 = $output_164_1;
    if ($output_164 ne q{} && !defined $output_printed_164) {
        print $output_164;
        if (!($output_164 =~ m{\n\z}msx)) {
            print "\n";
        }
    }
    if ( !$pipeline_success_164 ) { $main_exit_code = 1; }
    }

exit $main_exit_code;
