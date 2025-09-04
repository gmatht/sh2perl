#!/usr/bin/env perl

use strict;
use warnings;

# Control flow examples
if (-f "file.txt") {
    print "File exists\n";
} else {
    print "File does not exist\n";
}

for my $i (1..5) {
    print "Number: $i\n";
}

my $i = 1;
while ($i < 10) {
    print "Counter: $i\n";
    $i++;
}

sub greet {
    my ($name) = @_;
    print "Hello, $name!\n";
}

greet("World");

