#!/usr/bin/env perl

use strict;
use warnings;

# Indexed array examples

print "== Indexed arrays ==\n";
my @arr = qw(one two three);
print "$arr[1]\n";        # two
print scalar(@arr) . "\n"; # 3
for my $x (@arr) {
    printf "%s ", $x;
}
print "\n";

