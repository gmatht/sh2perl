#!/usr/bin/env perl

use strict;
use warnings;

# Process substitution with comm examples

print "== Process substitution with comm ==\n";
my @list1 = qw(a b);
my @list2 = qw(b c);
my %seen;
$seen{$_}++ for @list1;
for my $item (@list2) {
    if ($seen{$item}) {
        print "$item\n";
    }
}

