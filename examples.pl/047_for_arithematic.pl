#!/usr/bin/env perl

use strict;
use warnings;

my $j = 1;
for my $i (1..5) {
    $j *= $i;
}
print "$j\n";

