#!/usr/bin/env perl

use strict;
use warnings;

# Advanced parameter expansion examples

print "== Advanced parameter expansion ==\n";
my $path = "/tmp/file.txt";
print (split('/', $path))[-1] . "\n";  # file.txt
my @path_parts = split('/', $path);
pop @path_parts;
print join('/', @path_parts) . "\n";   # /tmp
my $s2 = "abba";
$s2 =~ s/b/X/g;
print "$s2\n";  # aXXa

