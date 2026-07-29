#!/usr/bin/env perl
use strict;
use warnings;
print "Hello, World!\n";
print "Single quoted\
";
print "String with \"escaped\" quotes\n";
print "String with 'single' quotes\n";
print "exit: ${\($? >> 8)}\n";

