#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '063_01_deeply_nested_arithmetic.sh';
my $a;
my $b;
my $c;
my $d;
my $e;
my $f;
my $g;
my $h;
my $i;
my $j;
my $k;
my $l;
my $m;
my $n;
my $result = eval { int( ($a + $b) * ($c - $d) / ($e % $f) + ($g ** $h) - ($i << $j) | ($k & $l) ^ ($m | $n) ) } // "";
say "Deeply nested arithmetic result: $result";
