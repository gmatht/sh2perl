#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 0 variables: []
# set -euo
# set pipefail
print("== Basic grep parameters ==\n");
my $output_1 = '';
my $output_total_1 = '';
$output_1 = "text with pattern\n";
my @grep_lines_1;
my $count_1 = 0;
for my $line (split(/\n/, $output_1)) {
    if ($line =~ /PATTERN/i) {
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
$output_2 = "line1\nline2\nline3";
my @grep_lines_2;
my $count_2 = 0;
for my $line (split(/\n/, $output_2)) {
    if ($line !~ /line2/) {
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
$output_3 = "match\nno match\nmatch again";
my $count_3 = 0;
for my $line (split(/\n/, $output_3)) {
    if ($line =~ /match/) {
        $count_3++;
    }
}
$output_3 = $count_3 . "\n";
$output_total_3 .= $output_3;
$output_3 = $output_total_3;
print($output_3);
print("== Context parameters ==\n");
my $output_4 = '';
my $output_total_4 = '';
$output_4 = "line1\nline2\nTARGET\nline4\nline5";
my @lines_4 = split(/\n/, $output_4);
my @result_lines_4;
for my $i (0..$#lines_4) {
    my $line = $lines_4[$i];
    if ($line =~ /TARGET/) {
        # Add 2 lines after match
        push @result_lines_4, $line;
        for my $j (1..2) {
            if ($i + $j <= $#lines_4) {
                push @result_lines_4, $lines_4[$i + $j];
            }
        }
    }
}
$output_4 = join("\n", @result_lines_4) . "\n";
$output_total_4 .= $output_4;
$output_4 = $output_total_4;
print($output_4);
my $output_5 = '';
my $output_total_5 = '';
$output_5 = "line1\nline2\nTARGET\nline4\nline5";
my @lines_5 = split(/\n/, $output_5);
my @result_lines_5;
for my $i (0..$#lines_5) {
    my $line = $lines_5[$i];
    if ($line =~ /TARGET/) {
        # Add 2 lines before match
        for my $j (1..2) {
            if ($i - $j >= 0) {
                push @result_lines_5, $lines_5[$i - $j];
            }
        }
        push @result_lines_5, $line;
    }
}
$output_5 = join("\n", @result_lines_5) . "\n";
$output_total_5 .= $output_5;
$output_5 = $output_total_5;
print($output_5);
my $output_6 = '';
my $output_total_6 = '';
$output_6 = "line1\nline2\nTARGET\nline4\nline5";
my @lines_6 = split(/\n/, $output_6);
my @result_lines_6;
for my $i (0..$#lines_6) {
    my $line = $lines_6[$i];
    if ($line =~ /TARGET/) {
        # Add 1 lines before and after match
        for my $j (1..1) {
            if ($i - $j >= 0) {
                push @result_lines_6, $lines_6[$i - $j];
            }
        }
        push @result_lines_6, $line;
        for my $j (1..1) {
            if ($i + $j <= $#lines_6) {
                push @result_lines_6, $lines_6[$i + $j];
            }
        }
    }
}
$output_6 = join("\n", @result_lines_6) . "\n";
$output_total_6 .= $output_6;
$output_6 = $output_total_6;
print($output_6);
print("== File handling parameters ==\n");
local *STDOUT; open(STDOUT, '>', 'temp_file.txt') or die "Cannot open file: $!\n";
print("content\n");
if (open(my $fh_1, '<', 'temp_file.txt')) {
    while (my $line = <$fh_1>) {
        if ($line =~ /content/) {
            print "temp_file.txt:$line";
        }
    }
    close($fh_1);
}
if (open(my $fh_2, '<', 'temp_file.txt')) {
    while (my $line = <$fh_2>) {
        if ($line =~ /content/) {
            print $line;
        }
    }
    close($fh_2);
}
my $found = 0;
if (open(my $fh_3, '<', 'temp_file.txt')) {
    while (my $line = <$fh_3>) {
        if ($line =~ /content/) {
            $found = 1;
            last;
        }
    }
    close($fh_3);
}
if ($found) { print "temp_file.txt" }
my $pipeline_result_7 = system('grep -L nonexistent temp_file.txt') == 0 || system('true') == 0;
print("== Output formatting parameters ==\n");
my $output_8 = '';
my $output_total_8 = '';
$output_8 = "text with pattern in it\n";
my @grep_lines_8;
my $count_8 = 0;
for my $line (split(/\n/, $output_8)) {
    if ($line =~ /pattern/) {
        push @grep_lines_8, $line;
        $count_8++;
    }
}
$output_8 = join("\n", @grep_lines_8) . "\n";
$output_total_8 .= $output_8;
$output_8 = $output_total_8;
print($output_8);
my $output_9 = '';
my $output_total_9 = '';
$output_9 = "text with pattern in it\n";
my @grep_lines_9;
my $count_9 = 0;
for my $line (split(/\n/, $output_9)) {
    if ($line =~ /pattern/) {
        my $offset = index($line, "pattern");
        push @grep_lines_9, "$offset:$line";
        $count_9++;
    }
}
$output_9 = join("\n", @grep_lines_9) . "\n";
$output_total_9 .= $output_9;
$output_9 = $output_total_9;
print($output_9);
my $output_10 = '';
my $output_total_10 = '';
$output_10 = "text with pattern in it\n";
my @grep_lines_10;
my $count_10 = 0;
for my $line (split(/\n/, $output_10)) {
    if ($line =~ /pattern/) {
        push @grep_lines_10, $line;
        $count_10++;
    }
}
$output_10 = join("\n", @grep_lines_10) . "\n";
$output_total_10 .= $output_10;
$output_10 = $output_total_10;
print($output_10);
print("== Recursive and include/exclude parameters ==\n");
mkdir('-p') or die "Cannot create directory: $!\n";
mkdir('test_dir') or die "Cannot create directory: $!\n";
local *STDOUT; open(STDOUT, '>', 'test_dir/file1.txt') or die "Cannot open file: $!\n";
print("pattern here\n");
local *STDOUT; open(STDOUT, '>', 'test_dir/file2.txt') or die "Cannot open file: $!\n";
print("no pattern\n");
if (open(my $fh_4, '<', 'test_dir')) {
    while (my $line = <$fh_4>) {
        if ($line =~ /pattern/) {
            print $line;
        }
    }
    close($fh_4);
}
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
use File::Glob;
my @files = glob('--exclude="*.bak"');
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
my $output_11 = '';
my $output_total_11 = '';
$output_11 = qx(grep -r pattern test_dir '--include="*.txt"');
$output_11 = scalar(split(/\n/, $output_11)) . "\n";
$output_total_11 .= $output_11;
$output_11 = $output_total_11;
print($output_11);
print("== Advanced parameters ==\n");
my $output_12 = '';
my $output_total_12 = '';
$output_12 = "match1\nmatch2\nmatch3\nmatch4";
my @grep_lines_12;
my $count_12 = 0;
for my $line (split(/\n/, $output_12)) {
    last if $count_12 >= 2;
    if ($line =~ /match/) {
        push @grep_lines_12, $line;
        $count_12++;
    }
}
$output_12 = join("\n", @grep_lines_12) . "\n";
$output_total_12 .= $output_12;
$output_12 = $output_total_12;
print($output_12);
my $output_13 = '';
my $output_total_13 = '';
$output_13 = "text with pattern in it\n";
my $found_13 = 0;
for my $line (split(/\n/, $output_13)) {
    if ($line =~ /pattern/) {
        $found_13 = 1;
        last;
    }
}
$output_13 = $found_13;
$output_total_13 .= $output_13;
$output_13 = '';
$output_13 = "found\n";
$output_total_13 .= $output_13;
$output_13 = '';
$output_13 = "not found\n";
$output_total_13 .= $output_13;
$output_13 = $output_total_13;
print($output_13);
my $output_14 = '';
my $output_total_14 = '';
$output_14 = qx(grep -Z -l pattern temp_file.txt);
$output_14 =~ tr/\\0/\\n/;
$output_total_14 .= $output_14;
$output_14 = $output_total_14;
print($output_14);
unlink('temp_file.txt');
do { my $rm_cmd = "rm -rf test_dir"; qx{$rm_cmd}; };