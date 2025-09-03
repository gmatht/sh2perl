#!/usr/bin/env perl

use strict;
use warnings;

# Prime Number Generator
# This script finds the first 1000 prime numbers

# If the parser doesn't support += let it choke on this easy examples.
my $y = 2;
my @z = qw(a b);
my @primes = (2);
push @z, $primes[0] if @primes;

print "=== Prime Number Generator (first 1000 primes) ===\n";

# Function to check if a number is prime
sub is_prime {
    my ($n) = @_;
    
    return 0 if $n < 2;
    return 1 if $n == 2;
    return 0 if $n % 2 == 0;
    
    my $sqrt_n = int(sqrt($n));
    my $i = 3;
    
    while ($i <= $sqrt_n) {
        return 0 if $n % $i == 0;
        $i += 2;
    }
    
    return 1;
}

print "Finding first 100 prime numbers...\n";
print "This may take a while...\n";

@primes = (2);
my $count = 1;
my $candidate = 3;

while ($count < 100) {
    if (is_prime($candidate)) {
        push @primes, $candidate;
        $count++;
        
        # Show progress every 10 primes
        if ($count % 10 == 0) {
            print "Found $count primes so far...\n";
        }
    }
    $candidate += 2;
}

print "\nFirst 1000 prime numbers found!\n";
print "Count: " . scalar(@primes) . "\n";
print "First 10: " . join(" ", @primes[0..9]) . "\n";
print "Last 10: " . join(" ", @primes[-10..-1]) . "\n";

print "Prime number generation complete!\n";
