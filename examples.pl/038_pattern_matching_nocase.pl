#!/usr/bin/env perl

use strict;
use warnings;

# Case-insensitive matching examples

print "== nocasematch ==\n";
my $word = "Foo";
if ($word =~ /^foo$/i) {
    print "ci-match\n";
}
