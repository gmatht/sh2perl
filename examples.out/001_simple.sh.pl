#!/usr/bin/env perl
use strict;
use warnings;
$PROGRAM_NAME = '001_simple.sh';
print "Hello, World!\n";
if (-f "test.txt") {
    print "File exists\n";
}
for my $i (1..5) {
    print $i, "\n";
}
