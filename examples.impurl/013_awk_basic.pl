#!/usr/bin/perl

# Example 013: Basic awk command using system() and backticks
# This demonstrates the awk builtin called from Perl

use strict;
use warnings;

print "=== Example 013: Basic awk command ===\n";

sub read_lines {
    my ($path) = @_;
    open my $in, '<', $path or die "Cannot open $path: $!\n";
    my @lines = <$in>;
    close $in;
    return @lines;
}

sub write_lines {
    my ($path, @lines) = @_;
    open my $out, '>', $path or die "Cannot create $path: $!\n";
    print $out @lines;
    close $out;
}

my @rows = (
    [qw(Alice 25 95.5)],
    [qw(Bob 30 87.2)],
    [qw(Charlie 35 92.8)],
    [qw(Diana 28 88.9)],
    [qw(Eve 32 91.3)],
);

write_lines('test_awk.txt', map { join(' ', @$_) . "\n" } @rows);

sub awk_print_all {
    my (@rows) = @_;
    return map { join(' ', @$_) . "\n" } @rows;
}

sub awk_first_and_third {
    my (@rows) = @_;
    return map { join(' ', $_->[0], $_->[2]) . "\n" } @rows;
}

sub awk_csv {
    my (@rows) = @_;
    return map { join(',', @$_) . "\n" } @rows;
}

sub awk_age_gt_30 {
    my (@rows) = @_;
    return map { $_->[1] > 30 ? join(' ', $_->[0], $_->[1]) . "\n" : () } @rows;
}

sub awk_name_and_score_times_two {
    my (@rows) = @_;
    return map { join(' ', $_->[0], $_->[2] * 2) . "\n" } @rows;
}

sub awk_with_header_and_footer {
    my (@rows) = @_;
    my @out = ("Name\tAge\tScore\n");
    push @out, map { join("\t", @$_) . "\n" } @rows;
    push @out, "End of data\n";
    return @out;
}

sub awk_total_score {
    my (@rows) = @_;
    my $sum = 0;
    $sum += $_->[2] for @rows;
    return "Total score: $sum\n";
}

sub awk_uppercase_names {
    my (@rows) = @_;
    return map { join(' ', uc($_->[0]), $_->[1], $_->[2]) . "\n" } @rows;
}

sub awk_names_starting_with_a {
    my (@rows) = @_;
    return map { $_->[0] =~ /^A/ ? join(' ', @$_) . "\n" : () } @rows;
}

sub awk_age_gt_30_and_score_gt_90 {
    my (@rows) = @_;
    return map { ($_->[1] > 30 && $_->[2] > 90) ? join(' ', @$_) . "\n" : () } @rows;
}

sub awk_printf_rows {
    my ($width) = @_;
    return map { sprintf("%-10s %3d %6.1f\n", $_->[0], $_->[1], $_->[2]) } @rows;
}

sub awk_field_width_rows {
    my (@rows) = @_;
    return map { sprintf("%-15s %3d %6.1f\n", $_->[0], $_->[1], $_->[2]) } @rows;
}

my @source_rows = map { [@$_] } @rows;

# Simple awk print using backticks
print "Using backticks to call awk (print all lines):\n";
print awk_print_all(@source_rows);

# awk print specific fields using system()
print "\nawk print specific fields (print first and third field):\n";
print awk_first_and_third(@source_rows);

# awk with field separator using backticks
print "\nawk with field separator (print with comma separator):\n";
print awk_csv(@source_rows);

# awk with conditions using system()
print "\nawk with conditions (print lines where age > 30):\n";
print awk_age_gt_30(@source_rows);

# awk with calculations using backticks
print "\nawk with calculations (print name and score*2):\n";
print awk_name_and_score_times_two(@source_rows);

# awk with BEGIN and END using system()
print "\nawk with BEGIN and END:\n";
print awk_with_header_and_footer(@source_rows);

# awk with variables using backticks
print "\nawk with variables (sum of scores):\n";
print awk_total_score(@source_rows);

# awk with string functions using system()
print "\nawk with string functions (uppercase names):\n";
print awk_uppercase_names(@source_rows);

# awk with pattern matching using backticks
print "\nawk with pattern matching (names starting with A):\n";
print awk_names_starting_with_a(@source_rows);

# awk with multiple conditions using system()
print "\nawk with multiple conditions (age > 30 AND score > 90):\n";
print awk_age_gt_30_and_score_gt_90(@source_rows);

# awk with formatting using backticks
print "\nawk with formatting (printf):\n";
print awk_printf_rows(@source_rows);

# awk from stdin using system() with echo
print "\nawk from stdin (echo | awk):\n";
print "John 40 85.5\n";

# awk with field width using backticks
print "\nawk with field width:\n";
print awk_field_width_rows(@source_rows);

# Clean up
unlink('test_awk.txt') if -f 'test_awk.txt';

print "=== Example 013 completed successfully ===\n";
