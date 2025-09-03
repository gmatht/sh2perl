#!/usr/bin/env perl

use strict;
use warnings;

# Associative array examples

print "== Associative arrays ==\n";
my %map;
$map{foo} = 'bar';
$map{answer} = 42;
$map{two} = "1 + 1";
print "$map{foo}\n";      # bar
print "$map{answer}\n";   # 42

# Show all keys and values
for my $k (sort keys %map) {
    print "$k => $map{$k}\n";
}
