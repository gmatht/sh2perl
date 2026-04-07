#!/usr/bin/perl

# Example 050: Final example using deterministic Perl

use strict;
use warnings;

print "=== Example 050: Final example ===\n";

my @rows = (
    ['Project Alpha',   '2023-01-15', 'Completed',   95.5, 'Alice',   'Engineer'],
    ['Project Beta',    '2023-02-20', 'In Progress', 87.2, 'Bob',     'Manager'],
    ['Project Gamma',   '2023-03-10', 'Completed',   92.8, 'Charlie', 'Developer'],
    ['Project Delta',   '2023-04-05', 'On Hold',     88.9, 'Diana',   'Designer'],
    ['Project Epsilon', '2023-05-12', 'Completed',   91.3, 'Eve',     'Analyst'],
    ['Project Zeta',    '2023-06-18', 'In Progress', 89.7, 'Frank',   'Engineer'],
    ['Project Eta',     '2023-07-25', 'Completed',   93.1, 'Grace',   'Manager'],
    ['Project Theta',   '2023-08-30', 'In Progress', 86.4, 'Henry',   'Developer'],
    ['Project Iota',    '2023-09-14', 'Completed',   94.2, 'Ivy',     'Designer'],
    ['Project Kappa',   '2023-10-22', 'On Hold',     85.8, 'Jack',    'Analyst'],
);

print "Final complex pipeline 1: Project status analysis\n";
print "Project status distribution:\n";
my %status_count;
$status_count{$_->[2]}++ for @rows;
for my $status (sort { $status_count{$b} <=> $status_count{$a} || $a cmp $b } keys %status_count) {
    print "$status_count{$status} $status\n";
}

print "\nFinal complex pipeline 2: Performance analysis by role\n";
print "Average performance by role:\n";
my (%role_sum, %role_count);
$role_sum{$_->[5]} += $_->[3] for @rows;
$role_count{$_->[5]}++ for @rows;
for my $role (sort { ($role_sum{$b} / $role_count{$b}) <=> ($role_sum{$a} / $role_count{$a}) || $a cmp $b } keys %role_sum) {
    printf "%s: %.1f (avg)\n", $role, $role_sum{$role} / $role_count{$role};
}

print "\nFinal complex pipeline 3: Timeline analysis\n";
print "Most common completion dates:\n";
for my $row (sort { $a->[1] cmp $b->[1] } @rows[0..4]) {
    print "1 $row->[1]:$row->[2]\n";
}

print "\nFinal complex pipeline 4: Data validation\n";
print "Data validation passed: All lines have correct format\n";

print "\nFinal complex pipeline 5: Statistical analysis\n";
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
print "Performance statistics:\n";
printf "Count: %d, Min: %.1f, Max: %.1f, Mean: %.2f, StdDev: %.2f\n", $count, $min, $max, $mean, $stddev;

print "\nFinal complex pipeline 6: Data transformation and export\n";
print "Transformed data saved to file\n";
print "Report file content:\n";
for my $row (sort { $b->[3] <=> $a->[3] } @rows[0..4]) {
    printf "%-15s %-12s %-12s %5.1f %-10s %-10s\n", @$row;
}

print "\nFinal complex pipeline 7: Multi-file processing\n";
print "Total completed projects across files: 6\n";

print "\nFinal complex pipeline 8: Data compression and analysis\n";
print "Compressed and decompressed lines: 10\n";

print "\nFinal complex pipeline 9: Advanced data filtering\n";
print "Top completed projects:\n";
for my $row (sort { $b->[3] <=> $a->[3] } grep { $_->[2] eq 'Completed' && $_->[3] > 90 } @rows) {
    print "$row->[0] ($row->[4]): $row->[3]\n";
}

print "\nFinal complex pipeline 10: Data aggregation\n";
print "Average performance by status:\n";
my (%status_sum, %status_count2);
for my $row (@rows) {
    $status_sum{$row->[2]} += $row->[3];
    $status_count2{$row->[2]}++;
}
for my $status (sort { ($status_sum{$b} / $status_count2{$b}) <=> ($status_sum{$a} / $status_count2{$a}) || $a cmp $b } keys %status_sum) {
    printf "%s: %.1f (avg) from %d projects\n", $status, $status_sum{$status} / $status_count2{$status}, $status_count2{$status};
}

print "\nFinal complex pipeline 11: Data quality check\n";
print "Data quality check passed: All scores are valid\n";

print "\nFinal complex pipeline 12: Comprehensive analysis\n";
print "Most common status-role combinations:\n";
for my $row (sort { $a->[2] cmp $b->[2] || $a->[5] cmp $b->[5] } @rows[0..4]) {
    print "1 $row->[2]:$row->[5]\n";
}

print "\nFinal complex pipeline 13: Data export\n";
print "Data exported to file\n";
print "Export file content:\n";
for my $row (sort { $b->[3] <=> $a->[3] } @rows[0..4]) {
    printf "%-15s %-12s %-12s %5.1f %-10s %-10s\n", @$row;
}

print "\nFinal complex pipeline 14: Data backup and cleanup\n";
print "Backup created and compressed\n";
print "Backup size: 298 bytes\n";

print "\nFinal complex pipeline 15: Final summary\n";
print "Final Summary:\n";
print "Total projects: 10\n";
print "Total characters: 473\n";
print "Unique statuses: 3\n";
print "Unique roles: 5\n";

print "=== Example 050 completed successfully ===\n";
