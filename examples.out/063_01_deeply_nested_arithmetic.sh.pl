#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 15 variables: ["result", "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n"]
my $result = 0;
my $a = 0;
my $b = 0;
my $c = 0;
my $d = 0;
my $e = 0;
my $f = 0;
my $g = 0;
my $h = 0;
my $i = 0;
my $j = 0;
my $k = 0;
my $l = 0;
my $m = 0;
my $n = 0;

$result = do { my $v = eval {  ($a + $b) * ($c - $d) / ($e % $f) + ($g ** $h) - ($i << $j) | ($k & $l) ^ ($m | $n)  }; $@ ? '' : $v };
$ENV{result} = $result;
print(("Deeply nested arithmetic result: " . $result) . "\n");