#!/usr/bin/env perl

use strict;
use warnings;

# Case modification in parameter expansion

print "== Case modification in parameter expansion ==\n";
my $name = "world";
print uc($name) . "\n";        # WORLD
print lc($name) . "\n";        # world
print ucfirst($name) . "\n";   # World
