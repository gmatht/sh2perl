#!/usr/bin/env perl
use strict;
use warnings;
print "== Argument count ==\n";
print scalar(@ARGV), "\n";
print "== Arguments ==\n";
for my $a (@ARGV) {
    print "Arg: $a\n";
}

