#!/usr/bin/env perl

use strict;
use warnings;

# Pattern matching and regex examples
# Demonstrates Perl pattern matching and regex

print "== Pattern and regex ==\n";
my $s = "file.txt";
if ($s =~ /\.txt$/) {
    print "pattern-match\n";
}
if ($s =~ /^file\.[a-z]+$/) {
    print "regex-match\n";
}

print "== extglob equivalent ==\n";
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

print "== nocasematch equivalent ==\n";
my $word = "Foo";
if ($word =~ /^foo$/i) {
    print "ci-match\n";
}

