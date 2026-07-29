#!/usr/bin/env perl
use strict;
use warnings;
$0 = '063_03_nested_command_substitutions.sh';
my $output = "Result: " . ("Nested: " . ("Deep: " . ("Level 4")));
print $output, "\n";
