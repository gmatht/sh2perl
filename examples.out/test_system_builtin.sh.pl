#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 2 variables: ["result1", "result2"]
my $result1 = 0;
my $result2 = 0;

print("Testing system calls with builtin commands\n");
$result1 = `ls -la`;
$ENV{result1} = $result1;
$result2 = `find . -name "*.txt"`;
$ENV{result2} = $result2;
print("Results:\n");
print($result1 . "\n");
print($result2 . "\n");