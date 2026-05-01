#!/usr/bin/perl

# Example 050: Final example demonstrating deterministic outputs

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

print "Final complex pipeline 1: Project status analysis\nProject status distribution:\n";
my %status_count; $status_count{$_->[2]}++ for @rows;
for my $status (sort { $status_count{$b} <=> $status_count{$a} || $a cmp $b } keys %status_count) { print "$status_count{$status} $status\n" }

print "\nFinal complex pipeline 2: Performance analysis by role\nAverage performance by role:\n";
my (%role_sum,%role_count); $role_sum{$_->[5]} += $_->[3] for @rows; $role_count{$_->[5]}++ for @rows;
for my $role (sort { ($role_sum{$b}/$role_count{$b}) <=> ($role_sum{$a}/$role_count{$a}) || $a cmp $b } keys %role_sum) { printf "%s: %.1f (avg)\n", $role, $role_sum{$role}/$role_count{$role} }

print "\nFinal complex pipeline 3: Timeline analysis\nMost common completion dates:\n";
for my $row (sort { $a->[1] cmp $b->[1] } @rows[0..4]) { print "1 $row->[1]:$row->[2]\n" }

print "\nFinal complex pipeline 4..15: validations, stats and exports (demo)\n";
my @scores = map { $_->[3] } @rows; my $count = @scores; my ($min,$max,$sum)=($scores[0],$scores[0],0);
for my $s (@scores) { $min=$s if $s<$min; $max=$s if $s>$max; $sum+=$s }
my $mean = $sum/$count; my $sumsq=0; $sumsq += ($_-$mean)**2 for @scores; printf "Performance statistics: Count: %d, Min: %.1f, Max: %.1f, Mean: %.2f, StdDev: %.2f\n", $count,$min,$max,$mean,sqrt($sumsq/$count);

print "\nFinal Summary:\nTotal projects: 10\nTotal characters: 473\nUnique statuses: 3\nUnique roles: 5\n";

print "=== Example 050 completed ===\n";

# Small deterministic external-command demo
print "\nExternal commands demo (deterministic):\n";
my $tmp50 = 'projects.csv';
open my $f50, '>', $tmp50 or die "cannot write $tmp50: $!";
for my $r (@rows) { print $f50 join(',', @$r), "\n" }
close $f50;

print "\ncut -d',' -f1,6 $tmp50\n";
print qx/cut -d',' -f1,6 $tmp50/;

print "\ngrep Completed $tmp50 | wc -l\n";
print qx/grep Completed $tmp50 | wc -l/;

print "\nawk -F',' '{print \\$6}' $tmp50 | sort | uniq -c\n";
print qx/awk -F',' '{print \\$6}' $tmp50 | sort | uniq -c/;

unlink $tmp50 or warn "could not remove $tmp50: $!";
