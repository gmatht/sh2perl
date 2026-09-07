#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
my $divisor;
my $factors;
my $n;

say join(' ', "=== Number Factorization Examples ===");
sub factorize {
    {
        $n = "\$1";
    };
    {
        $divisor = "2";
    };
    # TODO(unsupported): local
    print join(' ', "Factors of " . $n . ": ");
    while (($n > 1)) {
        while ((($n % $divisor) == 0)) {
            if ((($factors) eq "")) {
                $factors = $divisor;
            } else {
                $factors = $factors . " * " . $divisor;
            }
            $n = int(($n) / ($divisor));
        }
        $divisor = ($divisor + 1);
        if ((($divisor * $divisor) > $n)) {
            if (($n > 1)) {
                if ((($factors) eq "")) {
                    $factors = $n;
                } else {
                    $factors = $factors . " * " . $n;
                }
            }
            last;
        }
    }
    say join(' ', $factors);
}
factorize("12");
factorize("28");
factorize("100");
factorize("12345");
say join(' ', "Factorization complete!");
# 1 construct(s) lowered to TODO markers

