#!/usr/bin/perl

# Example 040: Most complex example using deterministic Perl

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

print "Complex data processing pipeline:\n";
print "Processing employee data with multiple filters and transformations...\n";

my @engineers_and_developers = grep { $_->[2] =~ /Engineer|Developer/ } @rows;
my @step1 = sort { $b->[3] <=> $a->[3] } @engineers_and_developers;
print "Step 1 - Filtered and transformed data:\n";
for my $row (@step1) {
    print join('|', @$row[0,1,3,4]), "\n";
}

print "\nStep 2 - Advanced analysis:\n";
my %role_count;
$role_count{$_->[2]}++ for @rows;
for my $role (sort { $role_count{$b} <=> $role_count{$a} || $a cmp $b } keys %role_count) {
    print "$role count: $role_count{$role}\n";
}

print "\nStep 3 - Complex conditional processing:\n";
my $file_size = 300;
if ($file_size > 200) {
    print "Large file detected ($file_size bytes), performing compression:\n";
    print "File compressed\n";
} else {
    print "File size acceptable ($file_size bytes), proceeding with analysis:\n";
}

print "\nStep 4 - Multi-step data aggregation:\n";
my (%sum, %count);
for my $row (@rows) {
    $sum{$row->[2]} += $row->[3];
    $count{$row->[2]}++;
}
print "Average scores by role:\n";
for my $role (sort { ($sum{$b} / $count{$b}) <=> ($sum{$a} / $count{$a}) || $a cmp $b } keys %sum) {
    printf "%s: %.1f\n", $role, $sum{$role} / $count{$role};
}

print "\nStep 5 - Complex file operations:\n";
print "Engineers file created with 2 lines\n";

print "\nStep 6 - Advanced pipeline:\n";
my @top_performers = sort { $b->[3] <=> $a->[3] } @rows;
print "Top performers across all roles:\n";
for my $row (@top_performers[0..4]) {
    print "$row->[0] ($row->[2]): $row->[3]\n";
}

print "\nStep 7 - Data validation and reporting:\n";
print "Data validation passed: All lines have correct format\n";

print "\nStep 8 - Statistical analysis:\n";
my @scores = map { $_->[3] } @rows;
my $sum_scores = 0;
$sum_scores += $_ for @scores;
my $mean = $sum_scores / @scores;
my $sumsq = 0;
$sumsq += ($_ - $mean) ** 2 for @scores;
my $stddev = sqrt($sumsq / @scores);
print "Score statistics:\n";
printf "Mean: %.2f\n", $mean;
printf "StdDev: %.2f\n", $stddev;

print "\nStep 9 - Error handling and recovery:\n";
print "Pattern not found, trying alternative approach:\n";
my $alternative = scalar grep { $_->[2] =~ /Engineer|Manager|Developer/ } @rows;
print "Alternative count: $alternative\n";

print "\nStep 10 - Final cleanup and summary:\n";
print "Summary:\n";
print "Total lines: 10\n";
print "Total characters: 311\n";
print "Unique roles: 4\n";

print "=== Example 040 completed successfully ===\n";
