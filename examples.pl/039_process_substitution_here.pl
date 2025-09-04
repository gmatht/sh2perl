#!/usr/bin/env perl

use strict;
use warnings;

# Here-string examples

print "== Here-string with grep -o ==\n";
my $text = "some pattern here";
if ($text =~ /(pattern)/) {
    print "$1\n";
}

