#!/usr/bin/perl

# Example 033: Demonstrate external nice utility (deterministic)

use strict;
use warnings;

print "=== Example 033: External nice utility ===\n";

{
    my $c = "nice -n 5 echo 'Nice priority 5'";
    print "\n$c\n";
    print qx/$c/;
}
{
    my $c = "nice echo 'Nice default priority'";
    print "\n$c\n";
    print qx/$c/;
}

print "\nNote: Changing niceness may require privileges; these examples just show
invocation and output deterministically.\n";

print "\n=== Example 033 completed ===\n";
