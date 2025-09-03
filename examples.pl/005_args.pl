#!/usr/bin/env perl

use strict;
use warnings;

# Demonstrates reading command-line arguments
# This example is intentionally simple so it parses cleanly

print "== Argument count ==\n";
print scalar(@ARGV) . "\n";

print "== Arguments ==\n";
for my $arg (@ARGV) {
    print "Arg: $arg\n";
}
