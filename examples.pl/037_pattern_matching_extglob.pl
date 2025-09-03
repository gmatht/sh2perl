#!/usr/bin/env perl

use strict;
use warnings;

# Extended glob examples

print "== extglob ==\n";
my $f1 = "file.js";
my $f2 = "thing.min.js";
if ($f1 =~ /\.js$/ && $f1 !~ /\.min\.js$/) {
    print "f1-ok\n";
}
if ($f2 =~ /\.js$/ && $f2 !~ /\.min\.js$/) {
    # This won't match
} else {
    print "f2-filtered\n";
}
