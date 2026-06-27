#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 0 variables: []
print("Hello, World!\n");
my $output_1 = '';
my $output_total_1 = '';
my @ls_files_1;
if (opendir(my $dh_1, '.')) {
    while (my $file = readdir($dh_1)) {
        next if $file eq '.' || $file eq '..';
        push @ls_files_1, $file;
    }
    closedir($dh_1);
}
@ls_files_1 = sort @ls_files_1;
$output_1 = join("\n", @ls_files_1);
my @grep_lines_1;
my $count_1 = 0;
for my $line (split(/\n/, $output_1)) {
    if ($line !~ /__tmp_test_output.pl/) {
        push @grep_lines_1, $line;
        $count_1++;
    }
}
$output_1 = join("\n", @grep_lines_1) . "\n";
$output_total_1 .= $output_1;
$output_1 = $output_total_1;
print($output_1);
print(`ls | grep -v __tmp_test_output.pl` . "\n");