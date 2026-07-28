#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';
use IPC::Open3;

my $output         = q{};
our $CHILD_ERROR;

$PROGRAM_NAME = '062_01_ambiguous_operators.sh';
say "Testing ambiguous operators...";
my $result = eval { int(2**3**2) } // "";
say "2**3**2 = $result";
