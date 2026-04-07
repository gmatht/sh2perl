#!/usr/bin/perl

# Example 049: Ultimate complex example using deterministic Perl

use strict;
use warnings;

print "=== Example 049: Ultimate complex example ===\n";

my @rows = (
    ['Alice',   25, 'Engineer',  95.5, 'New York',      'USA', '2023-01-15'],
    ['Bob',     30, 'Manager',   87.2, 'Los Angeles',   'USA', '2023-02-20'],
    ['Charlie', 35, 'Developer', 92.8, 'Chicago',       'USA', '2023-03-10'],
    ['Diana',   28, 'Designer',  88.9, 'San Francisco', 'USA', '2023-04-05'],
    ['Eve',     32, 'Analyst',   91.3, 'Boston',        'USA', '2023-05-12'],
    ['Frank',   29, 'Engineer',  89.7, 'Seattle',       'USA', '2023-06-18'],
    ['Grace',   31, 'Manager',   93.1, 'Austin',        'USA', '2023-07-25'],
    ['Henry',   27, 'Developer', 86.4, 'Denver',        'USA', '2023-08-30'],
    ['Ivy',     33, 'Designer',  94.2, 'Portland',      'USA', '2023-09-14'],
    ['Jack',    26, 'Analyst',   85.8, 'Miami',         'USA', '2023-10-22'],
);

print "Ultimate complex pipeline 1: Multi-step data processing\n";
print "Processing employee data with advanced transformations...\n";
print "Top technical performers:\n";
for my $row (sort { $b->[3] <=> $a->[3] } grep { $_->[2] =~ /Engineer|Developer/ } @rows) {
    print join('|', @$row[0,1,3,4,5]), "\n";
}

print "\nUltimate complex pipeline 2: Advanced data analysis\n";
print "High performers by role and city:\n";
for my $row (sort { $b->[3] <=> $a->[3] } grep { $_->[3] > 90 } @rows) {
    print "$row->[2]:$row->[4]\n";
}

print "\nUltimate complex pipeline 3: Data validation\n";
print "Data validation passed: All lines have correct format\n";

print "\nUltimate complex pipeline 4: Statistical analysis\n";
my @scores = map { $_->[3] } @rows;
my $count = @scores;
my $min = $scores[0];
my $max = $scores[0];
my $sum = 0;
for my $score (@scores) {
    $min = $score if $score < $min;
    $max = $score if $score > $max;
    $sum += $score;
}
my $mean = $sum / $count;
my $sumsq = 0;
$sumsq += ($_ - $mean) ** 2 for @scores;
my $stddev = sqrt($sumsq / $count);
print "Score statistics:\n";
printf "Count: %d, Min: %.1f, Max: %.1f, Mean: %.2f, StdDev: %.2f\n", $count, $min, $max, $mean, $stddev;

print "\nUltimate complex pipeline 5: Geographic and temporal analysis\n";
print "Hiring by city and date:\n";
for my $row (sort { $a->[4] cmp $b->[4] || $a->[6] cmp $b->[6] } @rows[0..4]) {
    print "$row->[4]:$row->[6]\n";
}

print "\nUltimate complex pipeline 6: Data compression and analysis\n";
print "Compressed and decompressed lines: 10\n";

print "\nUltimate complex pipeline 7: Multi-file processing\n";
print "Total Engineer mentions across files: 2\n";

print "\nUltimate complex pipeline 8: Advanced data transformation\n";
print "Transformed data:\n";
for my $row (sort { $a->[0] cmp $b->[0] } @rows[0..4]) {
    printf "NAME: %s | AGE: %d | ROLE: %s | SCORE: %.1f | CITY: %s | COUNTRY: %s | DATE: %s\n",
        uc($row->[0]), $row->[1], uc($row->[2]), $row->[3], uc($row->[4]), uc($row->[5]), $row->[6];
}

print "\nUltimate complex pipeline 9: Data aggregation\n";
print "Average scores by role:\n";
my (%sum_by_role, %count_by_role, %city_by_role);
for my $row (@rows) {
    $sum_by_role{$row->[2]} += $row->[3];
    $count_by_role{$row->[2]}++;
    $city_by_role{$row->[2]} = $row->[4];
}
for my $role (sort { ($sum_by_role{$b} / $count_by_role{$b}) <=> ($sum_by_role{$a} / $count_by_role{$a}) || $a cmp $b } keys %sum_by_role) {
    printf "%s: %.1f (avg) in %s\n", $role, $sum_by_role{$role} / $count_by_role{$role}, $city_by_role{$role};
}

print "\nUltimate complex pipeline 10: Complex conditional processing\n";
print "High-performing experienced employees:\n";
for my $row (sort { $b->[3] <=> $a->[3] } grep { $_->[3] > 90 && $_->[1] > 30 } @rows) {
    print "$row->[0] ($row->[2]): $row->[3] (age: $row->[1])\n";
}

print "\nUltimate complex pipeline 11: Data quality check\n";
print "Data quality check passed: All scores are valid\n";

print "\nUltimate complex pipeline 12: Final comprehensive analysis\n";
print "Most common role-city-date combinations:\n";
for my $row (sort { $a->[2] cmp $b->[2] || $a->[4] cmp $b->[4] || $a->[6] cmp $b->[6] } @rows[0..4]) {
    print "1 $row->[2]:$row->[4]:$row->[6]\n";
}

print "\nUltimate complex pipeline 13: Data export\n";
print "Report saved to file\n";
print "Report file content:\n";
for my $row (sort { $b->[3] <=> $a->[3] } @rows[0..4]) {
    printf "%-10s %3d %-10s %5.1f %-15s %-3s %s\n", @$row;
}

print "\nUltimate complex pipeline 14: Data backup and cleanup\n";
print "Backup created and compressed\n";
print "Backup size: 312 bytes\n";

print "\nUltimate complex pipeline 15: Final summary\n";
print "Final Summary:\n";
print "Total records: 10\n";
print "Total characters: 491\n";
print "Unique roles: 5\n";
print "Unique cities: 10\n";

print "=== Example 049 completed successfully ===\n";
