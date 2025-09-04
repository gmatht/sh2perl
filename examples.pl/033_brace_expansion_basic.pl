#!/usr/bin/env perl

use strict;
use warnings;

# Basic brace expansion examples

print "== Basic brace expansion ==\n";
print join(" ", 1..5) . "\n";
print join(" ", 'a'..'c') . "\n";
print join(" ", map { sprintf("%02d", $_) } 0, 2, 4) . "\n";

