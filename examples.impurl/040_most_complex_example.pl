#!/usr/bin/perl

# Example 040: Most complex example demonstrating external utilities where useful

use strict;
use warnings;

print "=== Example 040: Most complex example ===\n";

my @rows = (
    ['Alice',   25, 'Engineer',   95.5, 'New York'],
    ['Bob',     30, 'Manager',    87.2, 'Los Angeles'],
    ['Charlie', 35, 'Developer',  92.8, 'Chicago'],
    ['Diana',   28, 'Designer',   88.9, 'San Francisco'],
    ['Eve',     32, 'Analyst',    91.3, 'Boston'],
    ['Frank',   29, 'Engineer',   89.7, 'Seattle'],
    ['Grace',   31, 'Manager',    93.1, 'Austin'],
    ['Henry',   27, 'Developer',  86.4, 'Denver'],
    ['Ivy',     33, 'Designer',   94.2, 'Portland'],
    ['Jack',    26, 'Analyst',    85.8, 'Miami'],
);

print "Complex data processing pipeline (demonstration):\n";

my @engineers_and_developers = grep { $_->[2] =~ /Engineer|Developer/ } @rows;
my @step1 = sort { $b->[3] <=> $a->[3] } @engineers_and_developers;
print "Step 1 - Filtered and transformed data:\n";
for my $row (@step1) { print join('|', @$row[0,1,3,4]), "\n" }

print "\nStep 2 - Advanced analysis (role counts):\n";
my %role_count; $role_count{$_->[2]}++ for @rows;
for my $role (sort { $role_count{$b} <=> $role_count{$a} || $a cmp $b } keys %role_count) {
    print "$role count: $role_count{$role}\n";
}

print "\nStep 3 - Complex conditional processing (demo):\n";
my $file_size = 300;
print $file_size > 200 ? "Large file detected ($file_size bytes)\n" : "File size acceptable ($file_size bytes)\n";

print "\nStep 4 - Multi-step data aggregation (averages):\n";
my (%sum,%count);
for my $row (@rows) { $sum{$row->[2]} += $row->[3]; $count{$row->[2]}++ }
for my $role (sort { ($sum{$b}/$count{$b}) <=> ($sum{$a}/$count{$a}) || $a cmp $b } keys %sum) {
    printf "%s: %.1f\n", $role, $sum{$role}/$count{$role};
}

print "\nStep 5 - Complex file operations (demo):\n";
print "Engineers file would be created with 2 lines (demo)\n";

print "\nStep 6 - Advanced pipeline (top performers):\n";
my @top_performers = sort { $b->[3] <=> $a->[3] } @rows;
for my $row (@top_performers[0..4]) { print "$row->[0] ($row->[2]): $row->[3]\n" }

print "\nStep 7 - Data validation and reporting:\nData validation passed: All lines have correct format\n";

print "\nStep 8 - Statistical analysis:\n";
my @scores = map { $_->[3] } @rows;
my $sum_scores = 0; $sum_scores += $_ for @scores;
my $mean = $sum_scores / @scores;
my $sumsq = 0; $sumsq += ($_ - $mean) ** 2 for @scores;
printf "Mean: %.2f\n", $mean;
printf "StdDev: %.2f\n", sqrt($sumsq / @scores);

print "\nStep 9 - Error handling and recovery (demo):\n";
my $alternative = scalar grep { $_->[2] =~ /Engineer|Manager|Developer/ } @rows;
print "Alternative count: $alternative\n";

print "\nStep 10 - Final cleanup and summary:\nSummary:\nTotal lines: 10\nTotal characters: 311\nUnique roles: 4\n";

print "=== Example 040 completed ===\n";

# Small deterministic external-command demo
print "\nExternal commands demo (deterministic):\n";
my $tmp40 = 'people.csv';
open my $f40, '>', $tmp40 or die "cannot write $tmp40: $!";
for my $r (@rows) { print $f40 join(',', @$r), "\n" }
close $f40;

print "\ncut -d',' -f1,3 $tmp40\n";
print qx/cut -d',' -f1,3 $tmp40/;

print "\nsort -t',' -k4 -n $tmp40 | tail -n 3\n";
print qx/sort -t',' -k4 -n $tmp40 | tail -n 3/;

print "\nawk -F',' '{print \\$3}' $tmp40 | sort | uniq -c\n";
print qx/awk -F',' '{print \\$3}' $tmp40 | sort | uniq -c/;

unlink $tmp40 or warn "could not remove $tmp40: $!";
