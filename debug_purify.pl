#!/usr/bin/env perl
use strict;
use warnings;

print "Testing purify.pl --help via backticks...\n";

my $help_output = `perl purify.pl --help 2>&1`;
my $help_result = $? >> 8;

print "Exit code: $help_result\n";
print "Output length: " . length($help_output) . "\n";
print "Output:\n$help_output\n";

