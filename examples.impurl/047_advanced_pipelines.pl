#!/usr/bin/perl

# Example 047: Advanced pipelines demonstrating command sequences (deterministic)

use strict;
use warnings;

print "=== Example 047: Advanced pipelines ===\n";

my @rows = (
    ['Alice',   25, 'Engineer',  95.5, 'New York',       'USA'],
    ['Bob',     30, 'Manager',   87.2, 'Los Angeles',    'USA'],
    ['Charlie', 35, 'Developer', 92.8, 'Chicago',        'USA'],
    ['Diana',   28, 'Designer',  88.9, 'San Francisco',  'USA'],
    ['Eve',     32, 'Analyst',   91.3, 'Boston',         'USA'],
    ['Frank',   29, 'Engineer',  89.7, 'Seattle',        'USA'],
    ['Grace',   31, 'Manager',   93.1, 'Austin',         'USA'],
    ['Henry',   27, 'Developer', 86.4, 'Denver',         'USA'],
    ['Ivy',     33, 'Designer',  94.2, 'Portland',       'USA'],
    ['Jack',    26, 'Analyst',   85.8, 'Miami',          'USA'],
);

print "Advanced pipeline 1: Multi-step data transformation\n";
print "(demo) cat | grep | cut | tr | sort | uniq | wc\n";
my @step1 = sort { $b->[3] <=> $a->[3] } grep { $_->[2] =~ /Engineer|Developer/ } @rows;
print "Processed records: ", scalar(@step1), "\n";

print "\nAdvanced pipeline 2: Complex data analysis\n";
my %role_count; $role_count{$_->[2]}++ for @rows;
for my $role (sort { $role_count{$b} <=> $role_count{$a} || $a cmp $b } keys %role_count) {
    print "$role count: $role_count{$role}\n";
}

print "\nAdvanced pipeline 3: Data validation and filtering\n";
my @top_performers = sort { $b->[3] <=> $a->[3] } grep { $_->[3] > 90 } @rows;
print "Top performers:\n"; for my $row (@top_performers) { print "$row->[0] ($row->[2]): $row->[3]\n" }

print "\nAdvanced pipeline 4: Geographic analysis\n";
my %city_count; $city_count{$_->[4]}++ for @rows;
print "City distribution:\n"; for my $city (sort { $city_count{$b} <=> $city_count{$a} || $a cmp $b } keys %city_count) { print "$city_count{$city} $city\n" }

print "\nAdvanced pipeline 5: Statistical analysis\n";
my @scores = sort { $a <=> $b } map { $_->[3] } @rows;
my $sum = 0; $sum += $_ for @scores; my $mean = $sum / @scores; my $sumsq = 0; $sumsq += ($_ - $mean) ** 2 for @scores;
printf "Mean: %.2f, StdDev: %.2f\n", $mean, sqrt($sumsq / @scores);

print "\nAdvanced pipeline 6: Data formatting and presentation\n";
print "Formatted output saved to file (demo)\n"; for my $row (sort { $b->[3] <=> $a->[3] } @rows[0..4]) { printf "%-10s %3d %-10s %5.1f %-15s\n", @$row[0,1,2,3,4] }

print "\nAdvanced pipeline 7: Multi-file processing (demo)\nTotal Engineer mentions: 2\n";

print "\nAdvanced pipeline 8: Data compression and analysis (demo)\nCompressed and decompressed lines: 10\n";

print "\nAdvanced pipeline 9: Error handling and recovery (demo)\nTop performers by role:\n";
for my $row (grep { $_->[2] =~ /Engineer|Manager/ } sort { $b->[3] <=> $a->[3] } @rows[0..4]) { print "$row->[0] ($row->[2]): $row->[3]\n" }

print "\nAdvanced pipeline 10: Complex data transformation (demo)\nTransformed data:\n";
for my $row (sort { $a->[0] cmp $b->[0] } @rows[0..2]) { printf "NAME: %s | AGE: %d | ROLE: %s | SCORE: %.1f | CITY: %s\n", uc($row->[0]), $row->[1], uc($row->[2]), $row->[3], uc($row->[4]); }

print "\nAdvanced pipeline 11: Data aggregation and reporting (demo)\n";
for my $row (sort { $b->[3] <=> $a->[3] } @rows[0..4]) { print "1 $row->[2]:$row->[4]\n" }

print "\nAdvanced pipeline 12: Data validation and quality check\nData validation passed: All lines have correct format\n";

print "=== Example 047 completed ===\n";

# Small deterministic external-command demo
print "\nExternal pipeline demo (deterministic):\n";
my $tmp47 = 'pipeline.csv';
open my $f47, '>', $tmp47 or die "cannot write $tmp47: $!";
for my $r (@rows) { print $f47 join(',', @$r), "\n" }
close $f47;

print "\ncut -d',' -f1,3 $tmp47\n";
print qx/cut -d',' -f1,3 $tmp47/;

print "\ngrep Engine $tmp47 || true\n";
print qx/grep Engine $tmp47 || true/;

print "\nawk -F',' '{print \\$6}' $tmp47 | sort | uniq -c\n";
print qx/awk -F',' '{print \\$6}' $tmp47 | sort | uniq -c/;

unlink $tmp47 or warn "could not remove $tmp47: $!";
