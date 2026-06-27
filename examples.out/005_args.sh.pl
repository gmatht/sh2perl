#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 1 variables: ["a"]
my $a = 0;

print("== Argument count ==\n");
print(scalar(@ARGV) . "\n");
print("== Arguments ==\n");
for $a (@ARGV) {
    print(("Arg: " . $a) . "\n");
;
}