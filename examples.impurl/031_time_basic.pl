#!/usr/bin/perl

# Example 031: Demonstrate a deterministic timing example

use strict;
use warnings;

print "=== Example 031: Deterministic timing demo ===\n";

# Instead of using the shell's time builtin (output varies across shells),
# show a deterministic timing measurement using Perl's time and a short command.
my $start = time();
system('perl', '-e', 'sleep 0');
my $end = time();
printf "Elapsed (seconds, integer resolution): %d\n", $end - $start;

print "\nIf you want the system time output try: /usr/bin/time -p sleep 0\n";

print "=== Example 031 completed ===\n";
