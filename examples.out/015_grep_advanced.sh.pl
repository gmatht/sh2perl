#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 0 variables: []
my $output_1 = '';
my $output_total_1 = '';
$output_1 = "match1\nmatch2\nmatch3\nmatch4";
my @grep_lines_1;
my $count_1 = 0;
for my $line (split(/\n/, $output_1)) {
    last if $count_1 >= 2;
    if ($line =~ /match/) {
        push @grep_lines_1, $line;
        $count_1++;
    }
}
$output_1 = join("\n", @grep_lines_1) . "\n";
$output_total_1 .= $output_1;
$output_1 = $output_total_1;
print($output_1);
my $output_2 = '';
my $output_total_2 = '';
$output_2 = "text with pattern in it\n";
my @grep_lines_2;
my $count_2 = 0;
for my $line (split(/\n/, $output_2)) {
    if ($line =~ /pattern/) {
        my $offset = index($line, "pattern");
        push @grep_lines_2, "$offset:$line";
        $count_2++;
    }
}
$output_2 = join("\n", @grep_lines_2) . "\n";
$output_total_2 .= $output_2;
$output_2 = $output_total_2;
print($output_2);
local *STDOUT; open(STDOUT, '>', 'temp_file.txt') or die "Cannot open file: $!\n";
print("content\n");
if (open(my $fh_1, '<', 'temp_file.txt')) {
    while (my $line = <$fh_1>) {
        if ($line =~ /content/) {
            print $line;
        }
    }
    close($fh_1);
}
if (open(my $fh_2, '<', 'temp_file.txt')) {
    while (my $line = <$fh_2>) {
        if ($line =~ /content/) {
            print "temp_file.txt:$line";
        }
    }
    close($fh_2);
}
my $output_3 = '';
my $output_total_3 = '';
$output_3 = qx(grep -Z -l pattern temp_file.txt);
$output_3 =~ tr/\\0/\\n/;
$output_total_3 .= $output_3;
$output_3 = $output_total_3;
print($output_3);
my $output_4 = '';
my $output_total_4 = '';
$output_4 = "text with pattern in it\n";
my @grep_lines_4;
my $count_4 = 0;
for my $line (split(/\n/, $output_4)) {
    if ($line =~ /pattern/) {
        push @grep_lines_4, $line;
        $count_4++;
    }
}
$output_4 = join("\n", @grep_lines_4) . "\n";
$output_total_4 .= $output_4;
$output_4 = '';
$output_4 = "Color not supported\n";
$output_total_4 .= $output_4;
$output_4 = $output_total_4;
print($output_4);
my $pipeline_result_5 = system('grep -q pattern temp_file.txt') == 0 && (print("found\n")) || (print("not found\n"));
unlink('temp_file.txt') or die "Cannot remove file: $!\n";