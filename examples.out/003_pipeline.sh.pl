#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;
use File::Find;

# DEBUG: Collected 0 variables: []
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
    if ($line =~ /\.txt$/) {
        push @grep_lines_1, $line;
        $count_1++;
    }
}
$output_1 = join("\n", @grep_lines_1) . "\n";
$output_1 = scalar(split(/\n/, $output_1)) . "\n";
$output_total_1 .= $output_1;
$output_1 = $output_total_1;
print($output_1);
print("\n");
my $output_2 = '';
my $output_total_2 = '';
my $cat_content_2 = '';
if (open(my $fh_2, '<', 'file.txt')) {
    while (my $line = <$fh_2>) {
        $cat_content_2 .= $line;
    }
    close($fh_2);
} else {
    warn "cat: file.txt: No such file or directory";
}
$output_2 = $cat_content_2;
$output_2 = join("\n", sort { $a cmp $b } split(/\n/, $output_2)) . "\n";
my %count_2;
for my $line (split(/\n/, $output_2)) {
    $count_2{$line}++;
}
my @uniq_result_2;
for my $key (sort keys %count_2) {
    my $count_val = $count_2{$key};
    my $count_str = sprintf("%7s %s", $count_val, $key);
    push @uniq_result_2, $count_str;
}
$output_2 = join("\n", @uniq_result_2) . "\n";
$output_2 = join("\n", reverse(sort { ($a <=> $b) || ($a cmp $b) } split(/\n/, $output_2))) . "\n";
$output_total_2 .= $output_2;
$output_2 = $output_total_2;
print($output_2);
print("\n");
my $output_3 = '';
my $output_total_3 = '';
my @find_files_3;
require File::Find;
File::Find::find({wanted => sub { if ($_ =~ /^.*\.sh$/) { push @find_files_3, $File::Find::name; } }, no_chdir => 1}, '.');
$output_3 = join("\n", @find_files_3);
my @xargs_files_3;
for my $file (split(/\n/, $output_3)) {
    if ($file ne '') {
        # Use Perl's built-in file reading instead of system grep for cross-platform compatibility
        my $found = 0;
        if (open(my $fh, '<', $file)) {
            while (my $line = <$fh>) {
                if ($line =~ /function/) {
                    $found = 1;
                    last;
                }
            }
            close($fh);
        }
        if ($found) {
            push @xargs_files_3, $file;
        }
    }
}
$output_3 = join("\n", @xargs_files_3);
$output_3 =~ tr/\\\\\\\\\///d;
$output_total_3 .= $output_3;
$output_3 = $output_total_3;
print($output_3);
print("\n");
my $output_4 = '';
my $output_total_4 = '';
my $cat_content_4 = '';
if (open(my $fh_4, '<', 'file.txt')) {
    while (my $line = <$fh_4>) {
        $cat_content_4 .= $line;
    }
    close($fh_4);
} else {
    warn "cat: file.txt: No such file or directory";
}
$output_4 = $cat_content_4;
$output_4 =~ tr/a/b/;
my @grep_lines_4;
my $count_4 = 0;
for my $line (split(/\n/, $output_4)) {
    if ($line =~ /hello/) {
        push @grep_lines_4, $line;
        $count_4++;
    }
}
$output_4 = join("\n", @grep_lines_4) . "\n";
$output_total_4 .= $output_4;
$output_4 = $output_total_4;
print($output_4);
print("\n");
my $output_5 = '';
my $output_total_5 = '';
my $cat_content_5 = '';
if (open(my $fh_5, '<', 'file.txt')) {
    while (my $line = <$fh_5>) {
        $cat_content_5 .= $line;
    }
    close($fh_5);
} else {
    warn "cat: file.txt: No such file or directory";
}
$output_5 = $cat_content_5;
$output_5 = join("\n", sort { $a cmp $b } split(/\n/, $output_5)) . "\n";
my @grep_lines_5;
my $count_5 = 0;
for my $line (split(/\n/, $output_5)) {
    if ($line =~ /hello/) {
        push @grep_lines_5, $line;
        $count_5++;
    }
}
$output_5 = join("\n", @grep_lines_5) . "\n";
$output_total_5 .= $output_5;
$output_5 = $output_total_5;
print($output_5);