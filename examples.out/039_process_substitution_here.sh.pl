#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 0 variables: []
# set -euo
# set pipefail
print("== Here-string with grep -o ==\n");
my $here_string_content = "some pattern here";
# Redirect HereString not yet implemented
my @here_lines = split(/\n/, $here_string_content);
foreach my $line (@here_lines) {
    if ($line =~ /(pattern)/) {
        print "$1\n";
    }
}