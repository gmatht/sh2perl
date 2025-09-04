#!/usr/bin/env perl

use strict;
use warnings;

# Array examples - indexed and associative arrays
# Demonstrates basic array operations in Perl

print "== Indexed arrays ==\n";
my @arr = qw(one two three);
print "$arr[1]\n";        # two
print scalar(@arr) . "\n"; # 3
for my $x (@arr) {
    print "$x ";
}
print "\n";

print "== Associative arrays (hashes) ==\n";
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

