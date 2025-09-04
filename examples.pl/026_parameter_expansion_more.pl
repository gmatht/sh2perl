#!/usr/bin/env perl

use strict;
use warnings;

# More parameter expansion examples

print "== More parameter expansion ==\n";
my $var = "hello world";
$var =~ s/^hello//;
print "$var\n";      #  world
$var = "hello world";
$var =~ s/world$//;
print "$var\n";      # hello 
$var = "hello world";
$var =~ s/o/0/g;
print "$var\n";      # hell0 w0rld

