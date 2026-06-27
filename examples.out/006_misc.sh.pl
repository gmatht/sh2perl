#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 0 variables: []
print("== Subshell ==\n");
do {
    print("inside-subshell\n");
};
print("== Simple pipeline ==\n");
my $output_1 = '';
my $output_total_1 = '';
$output_1 = "alpha beta\n";
my @grep_lines_1;
my $count_1 = 0;
for my $line (split(/\n/, $output_1)) {
    if ($line =~ /beta/) {
        push @grep_lines_1, $line;
        $count_1++;
    }
}
$output_1 = join("\n", @grep_lines_1) . "\n";
$output_total_1 .= $output_1;
$output_1 = $output_total_1;
print($output_1);