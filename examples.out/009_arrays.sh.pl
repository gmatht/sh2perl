#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 3 variables: ["arr", "x", "k"]
my $arr = 0;
my $x = 0;
my $k = 0;

# set -euo
# set pipefail
print("== Indexed arrays ==\n");
my @arr = ("one", "two", "three");
print($arr[1] . "\n");
print(scalar(@arr) . "\n");
for $x (@arr) {
    printf("%s ", "${x}");
;
}
print("\n");
print("== Associative arrays ==\n");
my %map = ();
$map{foo} = "bar";
$map{answer} = "42";
$map{two} = "1 + 1";
print($map{foo} . "\n");
print($map{answer} . "\n");
my $output_1 = '';
my $output_total_1 = '';
for my $k (keys(%map)) {
$output_1 .= $k . " => " . $map{$k} . "\n";
}
$output_1 = join("\n", sort { $a cmp $b } split(/\n/, $output_1)) . "\n";
$output_total_1 .= $output_1;
$output_1 = $output_total_1;
print($output_1);