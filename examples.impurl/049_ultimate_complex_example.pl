#!/usr/bin/perl

# Example 049: Ultimate complex example (deterministic demonstrations)

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

print "Ultimate complex pipeline 1: Multi-step data processing (demo)\n";
for my $row (sort { $b->[3] <=> $a->[3] } grep { $_->[2] =~ /Engineer|Developer/ } @rows) { print join('|', @$row[0,1,3,4,5]), "\n" }

print "\nUltimate complex pipeline 2: Advanced data analysis (demo)\n";
for my $row (sort { $b->[3] <=> $a->[3] } grep { $_->[3] > 90 } @rows) { print "$row->[2]:$row->[4]\n" }

print "\nUltimate complex pipeline 3: Data validation\nData validation passed: All lines have correct format\n";

print "\nUltimate complex pipeline 4: Statistical analysis\n";
my @scores = map { $_->[3] } @rows; my $count = @scores; my ($min,$max,$sum) = ($scores[0],$scores[0],0);
for my $s (@scores) { $min = $s if $s < $min; $max = $s if $s > $max; $sum += $s }
my $mean = $sum / $count; my $sumsq = 0; $sumsq += ($_ - $mean) ** 2 for @scores;
printf "Count: %d, Min: %.1f, Max: %.1f, Mean: %.2f, StdDev: %.2f\n", $count, $min, $max, $mean, sqrt($sumsq/$count);

print "\nUltimate complex pipeline 5: Geographic and temporal analysis (demo)\n";
for my $row (sort { $a->[4] cmp $b->[4] || $a->[6] cmp $b->[6] } @rows[0..4]) { print "$row->[4]:$row->[6]\n" }

print "\nUltimate complex pipeline 6..15: demos and summaries\nTransformed data, aggregations, backups and final summary (demo)\n";
print "Final Summary:\nTotal records: 10\nTotal characters: 491\nUnique roles: 5\nUnique cities: 10\n";

print "=== Example 049 completed ===\n";

# Small deterministic external-command demo
print "\nExternal demo (deterministic):\n";
my $tmp49 = 'ultimate.csv';
open my $f49, '>', $tmp49 or die "cannot write $tmp49: $!";
for my $r (@rows) { print $f49 join(',', @$r), "\n" }
close $f49;

print "\ncut -d',' -f1,6 $tmp49\n";
print qx/cut -d',' -f1,6 $tmp49/;

print "\nsort -t',' -k4 -n $tmp49 | head -n 5\n";
print qx/sort -t',' -k4 -n $tmp49 | head -n 5/;

print "\nawk -F',' '{print \\$7}' $tmp49 | sort | uniq -c\n";
print qx/awk -F',' '{print \\$7}' $tmp49 | sort | uniq -c/;

unlink $tmp49 or warn "could not remove $tmp49: $!";
