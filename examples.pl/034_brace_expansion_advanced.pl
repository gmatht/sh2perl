#!/usr/bin/env perl

use strict;
use warnings;

# Advanced brace expansion examples

print "== Advanced brace expansion ==\n";
my @letters = qw(a b c);
my @numbers = qw(1 2 3);
for my $letter (@letters) {
    for my $number (@numbers) {
        print "$letter$number ";
    }
}
print "\n";

print join(" ", map { $_ * 2 - 1 } 1..5) . "\n";  # 1..10..2 equivalent
print join(" ", map { chr(ord('a') + $_ * 3) } 0..8) . "\n";  # a..z..3 equivalent
