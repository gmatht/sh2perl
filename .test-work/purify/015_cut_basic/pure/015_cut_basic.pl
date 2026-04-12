#!/usr/bin/perl


use strict;
use warnings;

print "=== Example 015: Basic cut command ===\n";

my @rows = (
    [qw(Alice 25 95.5 Engineer)],
    [qw(Bob 30 87.2 Manager)],
    [qw(Charlie 35 92.8 Developer)],
    [qw(Diana 28 88.9 Designer)],
    [qw(Eve 32 91.3 Analyst)],
);

sub row_csv {
    my ($row) = @_;
    return join(',', @$row);
}

sub row_space {
    my ($row) = @_;
    return join(' ', @$row);
}

sub fields {
    my ($row, @idx) = @_;
    return join(',', @{$row}[@idx]);
}

sub chars {
    my ($text, @positions) = @_;
    return join('', map { substr($text, $_, 1) } @positions);
}

print "Using backticks to call cut (cut by comma, field 1):\n";
print join('', map { row_csv($_) . "\n" } map { [$_->[0]] } @rows);

print "\ncut with multiple fields (fields 1 and 3):\n";
print join('', map { fields($_, 0, 2) . "\n" } @rows);

print "\ncut with range of fields (fields 1-3):\n";
print join('', map { fields($_, 0, 1, 2) . "\n" } @rows);

print "\ncut with character positions (characters 1-10):\n";
print join('', map { substr(row_csv($_), 0, 10) . "\n" } @rows);

print "\ncut with specific characters (characters 1,3,5):\n";
print join('', map { chars(row_csv($_), 0, 2, 4) . "\n" } @rows);

print "\ncut with complement (everything except field 2):\n";
print join('', map { join(',', $_->[0], $_->[2], $_->[3]) . "\n" } @rows);

print "\ncut with output delimiter:\n";
print join('', map { join(' | ', $_->[0], $_->[2]) . "\n" } @rows);

print "\ncut with only delimited (skip lines without delimiter):\n";
print join('', map { row_csv($_) . "\n" } @rows);

print "\ncut with bytes (first 20 bytes):\n";
print join('', map { substr(row_csv($_), 0, 20) . "\n" } @rows);

print "\ncut with different delimiter (space):\n";
my $space_line = 'Alice 25 95.5 Engineer';
my @space_fields = split / /, $space_line;
print join(' ', $space_fields[0], $space_fields[2]) . "\n";

print "\ncut from stdin (echo | cut):\n";
my $stdin_line = 'John,40,85.5,Manager';
my @stdin_fields = split /,/, $stdin_line;
print join(',', @stdin_fields[0, 1]) . "\n";

print "\ncut with field ranges (fields 2-4):\n";
print join('', map { fields($_, 1, 2, 3) . "\n" } @rows);

print "\ncut with character ranges (characters 5-15):\n";
print join('', map { substr(row_csv($_), 4, 11) . "\n" } @rows);

print "=== Example 015 completed successfully ===\n";
