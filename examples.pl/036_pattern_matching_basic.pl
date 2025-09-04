#!/usr/bin/env perl

use strict;
use warnings;

# Basic pattern matching examples

print "== Pattern and regex ==\n";
my $s = "file.txt";
if ($s =~ /\.txt$/) {
    print "pattern-match\n";
}
if ($s =~ /^file\.[a-z]+$/) {
    print "regex-match\n";
}

