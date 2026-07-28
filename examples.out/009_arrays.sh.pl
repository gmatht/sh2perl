#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use locale;
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '009_arrays.sh';
$__set_e = 1;
# set uo not implemented
# set pipefail not implemented
say "== Indexed arrays ==";
my @arr = ('one', 'two', 'three');
say $arr[1];
say scalar(@arr);
my $x;
for my $x (@arr) {
printf('%s ', "$x");
}
print "\n";
say "== Associative arrays ==";
my %map = ();
$map{"foo"} = 'bar';
$map{"answer"} = '42';
$map{"two"} = "1 + 1";
say $map{'foo'};
say $map{'answer'};
# Original bash: #!/usr/bin/env bash
do {
    my $output_142 = q{};
    my $output_printed_142;
    my $pipeline_success_142 = 1;
        $output_142 = q{};
    my @output_142_items = (keys %map);
    for my $k (@output_142_items) {
    $output_142 .= "$k => " . $map{$k}. "\n";
    }

        my @sort_lines_142_1 = split /\n/, $output_142;
    my @sort_sorted_142_1 = sort @sort_lines_142_1;
    my $output_142_1 = join("\n", @sort_sorted_142_1);
    $output_142 = $output_142_1;
    $output_142 = $output_142_1;
    if ($output_142 ne q{} && !defined $output_printed_142) {
        print $output_142;
        if (!($output_142 =~ m{\n\z})) {
            print "\n";
        }
    }
    if ( !$pipeline_success_142 ) { $main_exit_code = 1; }
    exit $main_exit_code if $__set_e && $main_exit_code != 0;
    }
