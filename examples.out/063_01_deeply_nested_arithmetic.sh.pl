#!/usr/bin/env perl
use strict;
use warnings;
use Carp;
use English qw( -no_match_vars );
use locale;
use IPC::Open3;

my $main_exit_code = 0;
my $ls_success = 0;
our $CHILD_ERROR;

my $result =  ($a + $b) * ($c - $d) / ($e % $f) + ($g ** $h) - ($i << $j) | ($k & $l) ^ ($m | $n) ;
print "Deeply nested arithmetic result: $result\n";
