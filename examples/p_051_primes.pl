#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
my $candidate;
my $count;
my $i;
my $n;
my $primes;
my $sqrt_n;
my $y;
my $z;
my @primes;
my @z;

$y = ($y += "2");
(push @z, "a", "b");
$z = ($z += @primes["0"..("0")+("1")-1]);
say join(' ', "=== Prime Number Generator (first 1000 primes) ===");
sub is_prime {
    {
        $n = "\$1";
    };
    if (($n < 2)) {
        return "1";
    }
    if (($n == 2)) {
        return "0";
    }
    if ((($n % 2) == 0)) {
        return "1";
    }
    # TODO(unsupported): local
    {
        $i = "3";
    };
    while (($i <= $sqrt_n)) {
        if ((($n % $i) == 0)) {
            return "1";
        }
        $i = ($i + 2);
    }
    return "0";
}
say join(' ', "Finding first 100 prime numbers...");
say join(' ', "This may take a while...");
(@primes = ("2"));
$count = "1";
$candidate = "3";
while (($count < 100)) {
    if (is_prime(join(' ', split(' ', $candidate)))) {
        (push @primes, join(' ', split(' ', $candidate)));
        $count = ($count + 1);
        if ((($count % 10) == 0)) {
            say join(' ', "Found " . $count . " primes so far...");
        }
    }
    $candidate = ($candidate + 2);
}
say join(' ', "");
say join(' ', "First 1000 prime numbers found!");
say join(' ', "Count: " . join(' ', scalar(@primes)));
say join(' ', "First 10: " . join(' ', substr($primes, "0", "10")));
say join(' ', "Last 10: " . join(' ', substr($primes, " -10")));
say join(' ', "Prime number generation complete!");
# 1 construct(s) lowered to TODO markers

