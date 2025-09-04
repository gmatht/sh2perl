#!/usr/bin/env perl

use strict;
use warnings;

# Number Factorization Calculator
# This script finds the prime factors of given numbers

print "=== Number Factorization Examples ===\n";

# Function to factorize a number
sub factorize {
    my ($n) = @_;
    my $divisor = 2;
    my @factors;
    
    print "Factors of $n: ";
    
    while ($n > 1) {
        while ($n % $divisor == 0) {
            push @factors, $divisor;
            $n = int($n / $divisor);
        }
        $divisor++;
        
        # Optimization: stop if divisor^2 > n
        if ($divisor * $divisor > $n) {
            if ($n > 1) {
                push @factors, $n;
            }
            last;
        }
    }
    
    print join(" * ", @factors) . "\n";
}

# Test with various numbers
factorize(12);
factorize(28);
factorize(100);
factorize(12345);

print "Factorization complete!\n";

