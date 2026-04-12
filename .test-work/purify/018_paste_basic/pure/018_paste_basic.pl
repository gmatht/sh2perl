#!/usr/bin/perl


use strict;
use warnings;

print "=== Example 018: Basic paste command ===\n";

my @names = qw(Alice Bob Charlie);
my @ages = qw(25 30 35);
my @jobs = qw(Engineer Manager Developer);

sub paste_two {
    my ($left, $right, $sep) = @_;
    $sep = "\t" unless defined $sep;
    my @out;
    for my $i (0 .. $#$left) {
        push @out, $left->[$i] . $sep . $right->[$i] . "\n";
    }
    return @out;
}

sub paste_three {
    my ($a, $b, $c, $sep1, $sep2) = @_;
    $sep1 = "\t" unless defined $sep1;
    $sep2 = "\t" unless defined $sep2;
    my @out;
    for my $i (0 .. $#$a) {
        push @out, $a->[$i] . $sep1 . $b->[$i] . $sep2 . $c->[$i] . "\n";
    }
    return @out;
}

sub paste_serial {
    my ($rows, $sep) = @_;
    $sep = "\t" unless defined $sep;
    return join($sep, @$rows) . "\n";
}

sub paste_single_char {
    my ($left, $right, $left_sep, $between_sep) = @_;
    $left_sep //= "\n";
    $between_sep //= "\t";
    my @out;
    for my $i (0 .. $#$left) {
        push @out, $left->[$i] . $left_sep . $right->[$i] . "\n";
    }
    return @out;
}

print "Using backticks to call paste (two files):\n";
print paste_two(\@names, \@ages);

print "\npaste with custom delimiter (-d ','):\n";
print paste_two(\@names, \@ages, ',');

print "\npaste with multiple files:\n";
print paste_three(\@names, \@ages, \@jobs);

print "\npaste with serial (-s):\n";
print paste_serial(\@names);

print "\npaste with newline delimiter (-d '\\n'):\n";
print paste_single_char(\@names, \@ages, "\n", "\n");

print "\npaste with tab delimiter (-d '\\t'):\n";
print paste_two(\@names, \@ages, "\t");

print "\npaste with zero delimiter (-d '\\0'):\n";
print paste_two(\@names, \@ages, "\0");

print "\npaste with space delimiter (-d ' '):\n";
print paste_two(\@names, \@ages, ' ');

print "\npaste with pipe delimiter (-d '|'):\n";
print paste_two(\@names, \@ages, '|');

print "\npaste from stdin (echo | paste):\n";
print paste_two([qw(Alice Bob)], \@ages);

print "\npaste with multiple delimiters:\n";
for my $i (0 .. $#names) {
    print $names[$i] . '|' . $ages[$i] . ',' . $jobs[$i] . "\n";
}

print "\npaste with serial and delimiter (-s -d ','):\n";
print paste_serial(\@names, ',');

print "=== Example 018 completed successfully ===\n";
