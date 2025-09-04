#!/usr/bin/env perl

use strict;
use warnings;

# Process substitution and here-strings
# Demonstrates advanced input/output handling in Perl

print "== Here-string with grep -o equivalent ==\n";
my $text = "some pattern here";
if ($text =~ /pattern/) {
    print "pattern\n";
}

print "== Process substitution with comm equivalent ==\n";
my @list1 = qw(a b);
my @list2 = qw(b c);
my %seen;
$seen{$_}++ for @list1;
for my $item (@list2) {
    if ($seen{$item}) {
        print "$item\n";
    }
}

print "== readarray/mapfile equivalent ==\n";
my @lines = qw(x y);
print join(" ", @lines) . "\n";

print "== More process substitution examples ==\n";
# Compare sorted outputs
my @list_a = sort qw(a c b);
my @list_b = sort qw(a b d);
if (join("\n", @list_a) ne join("\n", @list_b)) {
    print "Files differ\n";
}

# Use paste equivalent
my @names = qw(name1 name2);
my @values = qw(value1 value2);
for my $i (0..$#names) {
    print "$names[$i]\t$values[$i]\n";
}

