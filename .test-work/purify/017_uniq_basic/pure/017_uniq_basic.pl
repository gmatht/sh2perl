#!/usr/bin/perl


use strict;
use warnings;

print "=== Example 017: Basic uniq command ===\n";

my @lines = qw(apple banana apple cherry banana apple date cherry);

sub print_lines {
    print join('', map { "$_\n" } @_);
}

sub uniq_preserve_adjacent {
    my (@input) = @_;
    my @out;
    my $prev;
    for my $line (@input) {
        next if defined $prev && $line eq $prev;
        push @out, $line;
        $prev = $line;
    }
    return @out;
}

sub uniq_counts {
    my (@input) = @_;
    my @out;
    my ($prev, $count);
    for my $line (@input) {
        if (defined $prev && $line eq $prev) {
            $count++;
        } else {
            push @out, [ $prev, $count ] if defined $prev;
            $prev = $line;
            $count = 1;
        }
    }
    push @out, [ $prev, $count ] if defined $prev;
    return @out;
}

sub uniq_unique_only {
    my (@input) = @_;
    my %counts;
    $counts{$_}++ for @input;
    return grep { $counts{$_} == 1 } @input;
}

sub uniq_duplicate_only {
    my (@input) = @_;
    my %seen;
    my %dup;
    for my $line (@input) {
        if ($seen{$line}++) {
            $dup{$line} = 1;
        }
    }
    return sort grep { $dup{$_} } keys %dup;
}

sub uniq_ignore_case {
    my (@input) = @_;
    my @out;
    my $prev;
    for my $line (@input) {
        next if defined $prev && lc($line) eq lc($prev);
        push @out, $line;
        $prev = $line;
    }
    return @out;
}

sub uniq_skip_fields {
    my ($skip_fields, @input) = @_;
    my @out;
    my $prev_key;
    for my $line (@input) {
        my @parts = split / /, $line;
        my $key = join(' ', @parts[$skip_fields .. $#parts]);
        next if defined $prev_key && $key eq $prev_key;
        push @out, $line;
        $prev_key = $key;
    }
    return @out;
}

sub uniq_skip_chars {
    my ($skip_chars, @input) = @_;
    my @out;
    my $prev_key;
    for my $line (@input) {
        my $key = substr($line, $skip_chars);
        next if defined $prev_key && $key eq $prev_key;
        push @out, $line;
        $prev_key = $key;
    }
    return @out;
}

sub uniq_all_repeated {
    my (@input) = @_;
    my %counts;
    $counts{$_}++ for @input;
    return grep { $counts{$_} > 1 } @input;
}

sub uniq_group {
    my (@input) = @_;
    my @out;
    my $prev;
    for my $line (@input) {
        if (!defined $prev || $line ne $prev) {
            push @out, "\n" if @out;
            push @out, $line;
        } else {
            push @out, $line;
        }
        $prev = $line;
    }
    return @out;
}

sub uniq_with_field_separator {
    my (@input) = @_;
    my @out;
    my $prev_key;
    for my $line (@input) {
        my ($num, $word) = split /,/, $line;
        my $key = $word;
        next if defined $prev_key && $key eq $prev_key;
        push @out, $line;
        $prev_key = $key;
    }
    return @out;
}

sub uniq_width {
    my ($width, @input) = @_;
    my @out;
    my $prev_key;
    for my $line (@input) {
        my $key = substr($line, 0, $width);
        next if defined $prev_key && $key eq $prev_key;
        push @out, $line;
        $prev_key = $key;
    }
    return @out;
}

print "Using backticks to call uniq:\n";
print_lines(uniq_preserve_adjacent(@lines));

print "\nuniq with count (-c):\n";
for my $pair (uniq_counts(@lines)) {
    printf "%7d %s\n", $pair->[1], $pair->[0];
}

print "\nuniq with unique only (-u):\n";
print_lines(uniq_unique_only(@lines));

print "\nuniq with duplicate only (-d):\n";
print_lines(uniq_duplicate_only(@lines));

print "\nuniq with ignore case (-i):\n";
print_lines(uniq_ignore_case(@lines));

print "\nuniq with skip fields (-f 1):\n";
print_lines(uniq_skip_fields(1, qw(1 apple 2 banana 1 apple 3 cherry)));

print "\nuniq with skip characters (-s 2):\n";
print_lines(uniq_skip_chars(2, qw(aa1 bb2 aa3 cc4)));

print "\nuniq with check (-c):\n";
for my $pair (uniq_counts(@lines)) {
    printf "%7d %s\n", $pair->[1], $pair->[0];
}

print "\nuniq with all repeated (-D):\n";
print_lines(uniq_all_repeated(@lines));

print "\nuniq with group (-g):\n";
print_lines(uniq_group(@lines));

print "\nuniq from stdin (echo | uniq):\n";
print_lines(uniq_preserve_adjacent(qw(apple banana apple cherry)));

print "\nuniq with field separator:\n";
print_lines(uniq_with_field_separator('1,apple', '2,banana', '1,apple', '3,cherry'));

print "\nuniq with width (-w 3):\n";
print_lines(uniq_width(3, qw(abc abd abc def)));

print "=== Example 017 completed successfully ===\n";
