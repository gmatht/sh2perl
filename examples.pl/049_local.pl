#!/usr/bin/env perl

use strict;
use warnings;

my $a = 1;
print "$a\n";
{
    my $a = 2;
    print "$a\n";
}
print "$a\n";

