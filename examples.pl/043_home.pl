#!/usr/bin/env perl

use strict;
use warnings;

# Home directory examples
my $home = $ENV{HOME};
if ($home eq $home) {
    print "1\n";
} else {
    print "-\n";
}

if ("$home/Documents" eq $home) {
    print "2\n";
} else {
    print "-\n";
}

if ("$home/Documents" eq "$home/Documents") {
    print "3\n";
} else {
    print "-\n";
}

