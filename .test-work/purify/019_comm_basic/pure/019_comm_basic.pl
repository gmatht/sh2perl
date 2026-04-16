#!/usr/bin/perl
BEGIN { $0 = "/home/llm/src/sh2perl/examples.impurl/019_comm_basic.pl" }


use strict;
use warnings;

print "=== Example 019: Basic comm command ===\n";

my @left = qw(apple banana cherry date elderberry);
my @right = qw(banana cherry fig grape elderberry);

sub is_sorted {
    my ($items) = @_;
    for my $i (1 .. $#$items) {
        return 0 if $items->[$i - 1] gt $items->[$i];
    }
    return 1;
}

sub merge_rows {
    my ($left, $right) = @_;
    my @rows;
    my ($i, $j) = (0, 0);

    while ($i < @$left || $j < @$right) {
        my $l = $i < @$left ? $left->[$i] : undef;
        my $r = $j < @$right ? $right->[$j] : undef;

        if (!defined $r || (defined $l && $l lt $r)) {
            push @rows, [$l, '', ''];
            $i++;
        } elsif (!defined $l || $r lt $l) {
            push @rows, ['', $r, ''];
            $j++;
        } else {
            push @rows, ['', '', $l];
            $i++;
            $j++;
        }
    }

    return @rows;
}

sub render_rows {
    my ($rows, $delimiter, $suppress) = @_;
    $delimiter = "\t" unless defined $delimiter;
    $suppress ||= {};

    my @out;
    ROW: for my $row (@$rows) {
        my @cols = map { defined $_ ? $_ : '' } @$row;
        for my $idx (0 .. 2) {
            $cols[$idx] = '' if $suppress->{ $idx + 1 };
        }
        next ROW if $cols[0] eq '' && $cols[1] eq '' && $cols[2] eq '';
        push @out, join($delimiter, @cols) . "\n";
    }

    return @out;
}

sub count_rows {
    my ($rows) = @_;
    my ($left_only, $right_only, $common) = (0, 0, 0);
    for my $row (@$rows) {
        if ($row->[0] ne '') {
            $left_only++;
        } elsif ($row->[1] ne '') {
            $right_only++;
        } else {
            $common++;
        }
    }
    return ($left_only, $right_only, $common);
}

my @rows = merge_rows(\@left, \@right);

print "Using backticks to call comm:\n";
print render_rows(\@rows);

print "\ncomm with suppress column 1 (-1):\n";
print render_rows(\@rows, undef, { 1 => 1 });

print "\ncomm with suppress column 2 (-2):\n";
print render_rows(\@rows, undef, { 2 => 1 });

print "\ncomm with suppress column 3 (-3):\n";
print render_rows(\@rows, undef, { 3 => 1 });

print "\ncomm with suppress columns 1 and 2 (-12):\n";
print render_rows(\@rows, undef, { 1 => 1, 2 => 1 });

print "\ncomm with suppress columns 1 and 3 (-13):\n";
print render_rows(\@rows, undef, { 1 => 1, 3 => 1 });

print "\ncomm with suppress columns 2 and 3 (-23):\n";
print render_rows(\@rows, undef, { 2 => 1, 3 => 1 });

print "\ncomm with suppress all columns (-123):\n";
print render_rows(\@rows, undef, { 1 => 1, 2 => 1, 3 => 1 });

print "\ncomm with delimiter (-d ','):\n";
print render_rows(\@rows, ',', {});

print "\ncomm with check (check if files are sorted):\n";
print is_sorted(\@left) && is_sorted(\@right)
    ? "Files are sorted\n"
    : "Files are not sorted\n";

print "\ncomm with total (-t):\n";
my ($left_only, $right_only, $common) = count_rows(\@rows);
print "left-only=$left_only right-only=$right_only common=$common\n";

print "\ncomm with zero delimiter (-z):\n";
print render_rows(\@rows, "\0", {});

print "=== Example 019 completed successfully ===\n";
