#!/usr/bin/env perl

use strict;
use warnings;

# This script demonstrates basic Perl functionality
print "Hello, World!\n";

# Valid if statement
if (-f "test.txt") {
    print "File exists\n";
}

# Valid for loop
for my $i (1..5) {
    print "$i\n";
}

# Note: Perl doesn't leave $i as 5 after the loop like Bash does
# The variable $i is scoped to the for loop

# Only use basename if actually needed
# Note: Perl equivalent would be File::Basename::basename()

# "Hello, World!\n" is simpler in Perl
