#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw(-no_match_vars $ERRNO $EVAL_ERROR $INPUT_RECORD_SEPARATOR $OS_ERROR $PROGRAM_NAME);
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
print "Deeply nested arithmetic result: $result\n";

