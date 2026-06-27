#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 0 variables: []
my $output_1 = '';
my $output_total_1 = '';
$output_1 = "line1\nline2\nTARGET\nline4\nline5";
my @lines_1 = split(/\n/, $output_1);
my @result_lines_1;
for my $i (0..$#lines_1) {
    my $line = $lines_1[$i];
    if ($line =~ /TARGET/) {
        # Add 2 lines after match
        push @result_lines_1, $line;
        for my $j (1..2) {
            if ($i + $j <= $#lines_1) {
                push @result_lines_1, $lines_1[$i + $j];
            }
        }
    }
}
$output_1 = join("\n", @result_lines_1) . "\n";
$output_total_1 .= $output_1;
$output_1 = $output_total_1;
print($output_1);
my $output_2 = '';
my $output_total_2 = '';
$output_2 = "line1\nline2\nTARGET\nline4\nline5";
my @lines_2 = split(/\n/, $output_2);
my @result_lines_2;
for my $i (0..$#lines_2) {
    my $line = $lines_2[$i];
    if ($line =~ /TARGET/) {
        # Add 2 lines before match
        for my $j (1..2) {
            if ($i - $j >= 0) {
                push @result_lines_2, $lines_2[$i - $j];
            }
        }
        push @result_lines_2, $line;
    }
}
$output_2 = join("\n", @result_lines_2) . "\n";
$output_total_2 .= $output_2;
$output_2 = $output_total_2;
print($output_2);
my $output_3 = '';
my $output_total_3 = '';
$output_3 = "line1\nline2\nTARGET\nline4\nline5";
my @lines_3 = split(/\n/, $output_3);
my @result_lines_3;
for my $i (0..$#lines_3) {
    my $line = $lines_3[$i];
    if ($line =~ /TARGET/) {
        # Add 1 lines before and after match
        for my $j (1..1) {
            if ($i - $j >= 0) {
                push @result_lines_3, $lines_3[$i - $j];
            }
        }
        push @result_lines_3, $line;
        for my $j (1..1) {
            if ($i + $j <= $#lines_3) {
                push @result_lines_3, $lines_3[$i + $j];
            }
        }
    }
}
$output_3 = join("\n", @result_lines_3) . "\n";
$output_total_3 .= $output_3;
$output_3 = $output_total_3;
print($output_3);
print("Creating test files...\n");
local *STDOUT; open(STDOUT, '>', 'temp_file1.txt') or die "Cannot open file: $!\n";
print("pattern in file1\n");
local *STDOUT; open(STDOUT, '>', 'temp_file2.txt') or die "Cannot open file: $!\n";
print("no pattern in file2\n");
local *STDOUT; open(STDOUT, '>', 'temp_file3.txt') or die "Cannot open file: $!\n";
print("pattern in file3\n");
print("Recursive search results:\n");
use File::Glob;
my @files = glob('--include="*.txt"');
my @grep_results;
for my $file (@files) {
    if (-f $file) {
        if (open(my $fh, '<', $file)) {
            while (my $line = <$fh>) {
            if ($line =~ /pattern/) {
                print "$file:$line";
            }
        }
        close($fh);
    }
}
print("Result" . "2" . ".." . "." . "\n");
my $output_4 = '';
my $output_total_4 = '';
$output_4 = qx(grep -l pattern '*' .txt);
$output_4 = join("\n", sort { $a cmp $b } split(/\n/, $output_4)) . "\n";
$output_total_4 .= $output_4;
$output_4 = $output_total_4;
print($output_4);
print("Result" . "3" . ".." . "." . "\n");
my $found = 0;
if (open(my $fh_1, '<', '*')) {
    while (my $line = <$fh_1>) {
        if ($line =~ /pattern/) {
            $found = 1;
            last;
        }
    }
    close($fh_1);
}
if (!$found) { print "*" }
opendir(my $dh_2, '.') or die "Cannot open directory: $!\n";
while (my $file = readdir($dh_2)) {
if ($file =~ /^^temp\_file.*\.txt$$/) {
unlink($file) or die "Cannot remove file: $!\n";
}
}
closedir($dh_2);