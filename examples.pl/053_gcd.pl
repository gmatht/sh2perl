#!/usr/bin/env perl

use strict;
use warnings;

# Greatest Common Divisor Calculator
# This script calculates GCD using Euclidean algorithm

print "=== Greatest Common Divisor Examples ===\n";

# Function to calculate GCD using Euclidean algorithm
sub gcd {
    my ($a, $b) = @_;
    
    while ($b != 0) {
        my $temp = $b;
        $b = $a % $b;
        $a = $temp;
    }
    
    return $a;
}

# Test with various number pairs
print "GCD calculations:\n";

# Test case 1: 48 and 18
my $result = gcd(48, 18);
print "gcd(48, 18) = $result\n";

# Test case 2: 54 and 24
$result = gcd(54, 24);
print "gcd(54, 24) = $result\n";

# Test case 3: 7 and 13 (coprime)
$result = gcd(7, 13);
print "gcd(7, 13) = $result\n";

# Test case 4: 100 and 25
$result = gcd(100, 25);
print "gcd(100, 25) = $result\n";

# Test case 5: 12345 and 67890
$result = gcd(12345, 67890);
print "gcd(12345, 67890) = $result\n";

# Interactive mode
print "\nEnter two numbers to calculate their GCD (or press Ctrl+C to exit):\n";
while (1) {
    print "First number: ";
    my $num1 = <STDIN>;
    chomp $num1;
    
    last if $num1 eq "";
    
    print "Second number: ";
    my $num2 = <STDIN>;
    chomp $num2;
    
    last if $num2 eq "";
    
    if ($num1 =~ /^\d+$/ && $num2 =~ /^\d+$/) {
        $result = gcd($num1, $num2);
        print "gcd($num1, $num2) = $result\n";
    } else {
        print "Please enter valid positive integers.\n";
    }
    
    print "\n";
}

print "GCD calculation complete!\n";
