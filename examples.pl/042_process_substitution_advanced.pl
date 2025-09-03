#!/usr/bin/env perl

use strict;
use warnings;

# Advanced process substitution examples

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
