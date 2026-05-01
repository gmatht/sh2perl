#!/usr/bin/perl

# Example 018: Demonstrate external paste

use strict;
use warnings;

print "=== Example 018: External paste utility ===\n";

my $a = "test_a.txt";
my $b = "test_b.txt";
open my $fa, '>', $a or die "cannot write $a: $!";
open my $fb, '>', $b or die "cannot write $b: $!";
print $fa "Alice\nBob\nCharlie\n";
print $fb "25\n30\n35\n";
close $fa;
close $fb;

{
    my $cmd = "paste $a $b";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "paste -d',' $a $b";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "paste -s $a";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "printf '%s\\n' Alice Bob | paste - -d',' $b";
    print "\n$cmd\n";
    print `$cmd`;
}

unlink $a or warn "could not remove $a: $!";
unlink $b or warn "could not remove $b: $!";

print "\n=== Example 018 completed ===\n";
