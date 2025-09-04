#!/usr/bin/env perl

use strict;
use warnings;

# Parameter expansion examples
# Demonstrates advanced string manipulation in Perl

print "== Case modification ==\n";
my $name = "world";
print uc($name) . "\n";        # WORLD
print lc($name) . "\n";        # world
print ucfirst($name) . "\n";   # World

print "== Advanced string manipulation ==\n";
my $path = "/tmp/file.txt";
print (split('/', $path))[-1] . "\n";  # file.txt
my @path_parts = split('/', $path);
pop @path_parts;
print join('/', @path_parts) . "\n";   # /tmp
my $s2 = "abba";
$s2 =~ s/b/X/g;
print "$s2\n";  # aXXa

print "== More string manipulation ==\n";
my $var = "hello world";
$var =~ s/^hello//;
print "$var\n";      #  world
$var = "hello world";
$var =~ s/world$//;
print "$var\n";      # hello 
$var = "hello world";
$var =~ s/o/0/g;
print "$var\n";      # hell0 w0rld

print "== Default values ==\n";
my $maybe;
print defined($maybe) ? $maybe : "default" . "\n";  # default
$maybe = defined($maybe) ? $maybe : "default";
print "$maybe\n";  # default (and sets maybe)
if (!defined($maybe)) {
    die "error\n";
}

