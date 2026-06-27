#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 1 variables: ["result"]
my $result = 0;

print("Testing ambiguous operators...\n");
$result = do { my $v = eval { 2**3**2 }; $@ ? '' : $v };
$ENV{result} = $result;
print(("2**3**2 = " . $result) . "\n");