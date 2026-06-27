#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 0 variables: []
my $pipeline_result_1 = system('grep pattern /dev/null') == 0 || (print("No matches found\n"));
my $output_2 = '';
my $output_total_2 = '';
$output_2 = "HELLO world\n";
my @grep_lines_2;
my $count_2 = 0;
for my $line (split(/\n/, $output_2)) {
    if ($line =~ /hello/i) {
        push @grep_lines_2, $line;
        $count_2++;
    }
}
$output_2 = join("\n", @grep_lines_2) . "\n";
$output_total_2 .= $output_2;
$output_2 = $output_total_2;
print($output_2);
my $output_3 = '';
my $output_total_3 = '';
$output_3 = "line1\nline2\nline3";
my @grep_lines_3;
my $count_3 = 0;
for my $line (split(/\n/, $output_3)) {
    if ($line !~ /line2/) {
        push @grep_lines_3, $line;
        $count_3++;
    }
}
$output_3 = join("\n", @grep_lines_3) . "\n";
$output_total_3 .= $output_3;
$output_3 = $output_total_3;
print($output_3);
my $output_4 = '';
my $output_total_4 = '';
$output_4 = "first\nsecond\nthird";
my @grep_lines_4;
my $count_4 = 0;
for my $line (split(/\n/, $output_4)) {
    if ($line =~ /second/) {
        push @grep_lines_4, $line;
        $count_4++;
    }
}
$output_4 = join("\n", @grep_lines_4) . "\n";
$output_total_4 .= $output_4;
$output_4 = $output_total_4;
print($output_4);
my $output_5 = '';
my $output_total_5 = '';
$output_5 = "match\nno match\nmatch again";
my $count_5 = 0;
for my $line (split(/\n/, $output_5)) {
    if ($line =~ /match/) {
        $count_5++;
    }
}
$output_5 = $count_5 . "\n";
$output_total_5 .= $output_5;
$output_5 = $output_total_5;
print($output_5);
my $output_6 = '';
my $output_total_6 = '';
$output_6 = "text with pattern123 in it\n";
my @grep_lines_6;
my $count_6 = 0;
for my $line (split(/\n/, $output_6)) {
    if ($line =~ /pattern[0-9]\+/) {
        push @grep_lines_6, $line;
        $count_6++;
    }
}
$output_6 = join("\n", @grep_lines_6) . "\n";
$output_total_6 .= $output_6;
$output_6 = $output_total_6;
print($output_6);