#!/usr/bin/env perl

use strict;
use warnings;

# Loop examples
for my $i (1..5) {
    print "Number: $i\n";
}

my $j = 0;
for my $i (1..3) {
    $j++;
}
print "$j\n";

my $i = 1;
while ($i < 10) {
    print "Counter: $i\n";
    $i++;
}
