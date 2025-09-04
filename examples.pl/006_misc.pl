#!/usr/bin/env perl

use strict;
use warnings;

print "== Subshell ==\n";
# Perl doesn't have subshells like bash, but we can use system() or backticks
system('echo inside-subshell');

print "== Simple pipeline ==\n";
# echo "alpha beta" | grep beta
my $text = "alpha beta";
print "$text\n" if $text =~ /beta/;

