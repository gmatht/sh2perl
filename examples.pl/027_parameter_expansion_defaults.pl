#!/usr/bin/env perl

use strict;
use warnings;

# Default values in parameter expansion

print "== Default values ==\n";
my $maybe;
print defined($maybe) ? $maybe : "default" . "\n";  # default
$maybe = defined($maybe) ? $maybe : "default";
print "$maybe\n";  # default (and sets maybe)
if (!defined($maybe)) {
    die "error\n";
}
