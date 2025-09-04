#!/usr/bin/env perl

use strict;
use warnings;

# Fibonacci Sequence Calculator
# This script calculates and displays the first 20 Fibonacci numbers

print "=== Fibonacci Sequence (first 20 numbers) ===\n";

# Initialize first two numbers
my $a = 0;
my $b = 1;

print "Fibonacci numbers:\n";
print "$a $b ";

# Calculate next 18 numbers
for my $i (3..20) {
    my $temp = $a + $b;
    print "$temp ";
    $a = $b;
    $b = $temp;
}

print "\nDone!\n";

