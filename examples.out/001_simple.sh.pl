#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 1 variables: ["i"]
my $i = 0;

print("Hello, World!\n");
if (-f "test.txt") {
        print("File exists\n");
;
}
for $i (1..5) {
    print($i . "\n");
;
}