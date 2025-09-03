#!/usr/bin/env perl

use strict;
use warnings;

# Comprehensive Numeric Computation Examples
# This script demonstrates various mathematical algorithms

print "=== Comprehensive Numeric Computation Examples ===\n";
print "\n";

# Function to calculate Fibonacci numbers
sub fibonacci {
    my ($n) = @_;
    my $a = 0;
    my $b = 1;
    
    return $n if $n <= 1;
    
    for my $i (2..$n) {
        my $temp = $a + $b;
        $a = $b;
        $b = $temp;
    }
    
    return $b;
}

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

# Function to find first N primes
sub find_primes {
    my ($count) = @_;
    my @primes = (2);
    my $found = 1;
    my $candidate = 3;
    
    print "Finding first $count prime numbers...\n";
    
    while ($found < $count) {
        if (is_prime($candidate)) {
            push @primes, $candidate;
            $found++;
            
            # Show progress every 100 primes
            if ($found % 100 == 0) {
                print "Found $found primes so far...\n";
            }
        }
        $candidate += 2;
    }
    
    print "First $count primes found!\n";
    print "First 10: " . join(" ", @primes[0..9]) . "\n";
    print "Last 10: " . join(" ", @primes[-10..-1]) . "\n";
}

# Function to calculate GCD
sub gcd {
    my ($a, $b) = @_;
    
    while ($b != 0) {
        my $temp = $b;
        $b = $a % $b;
        $a = $temp;
    }
    
    return $a;
}

# Function to calculate LCM (Least Common Multiple)
sub lcm {
    my ($a, $b) = @_;
    my $gcd_result = gcd($a, $b);
    return int($a * $b / $gcd_result);
}

# Performance measurement function
sub measure_time {
    my ($code) = @_;
    my $start_time = time();
    eval $code;
    my $end_time = time();
    my $duration = $end_time - $start_time;
    print "Duration: ${duration}000 ms\n";
}

print "1. Fibonacci Sequence (first 20 numbers):\n";
my @fib_numbers;
for my $i (0..19) {
    push @fib_numbers, fibonacci($i);
}
print "   " . join(" ", @fib_numbers) . "\n";
print "\n";

print "2. Number Factorization:\n";
factorize(12);
factorize(28);
factorize(100);
factorize(12345);
print "\n";

print "3. Prime Number Generation:\n";
find_primes(100);  # Reduced to 100 for faster execution
print "\n";

print "4. Greatest Common Divisor Examples:\n";
print "   gcd(48, 18) = " . gcd(48, 18) . "\n";
print "   gcd(54, 24) = " . gcd(54, 24) . "\n";
print "   gcd(7, 13) = " . gcd(7, 13) . "\n";
print "   gcd(100, 25) = " . gcd(100, 25) . "\n";
print "   gcd(12345, 67890) = " . gcd(12345, 67890) . "\n";
print "\n";

print "5. Least Common Multiple Examples:\n";
print "   lcm(12, 18) = " . lcm(12, 18) . "\n";
print "   lcm(15, 20) = " . lcm(15, 20) . "\n";
print "   lcm(8, 12) = " . lcm(8, 12) . "\n";
print "\n";

print "6. Performance Benchmarks:\n";
print "   Computing fibonacci(30):\n";
measure_time("fibonacci(30)");

print "   Factorizing 12345:\n";
measure_time("factorize(12345)");

print "   Finding first 50 primes:\n";
measure_time("find_primes(50)");

print "\n=== All numeric computations complete! ===\n";
print "\nYou can now test these scripts with your translator:\n";
print "  ./sh2perl parse --perl examples/fibonacci.sh\n";
print "  ./sh2perl parse --rust examples/factorize.sh\n";
print "  ./sh2perl parse --python examples/primes.sh\n";
print "  ./sh2perl parse --lua examples/gcd.sh\n";
print "  ./sh2perl parse --js examples/numeric_computations.sh\n";
