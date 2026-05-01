#!/usr/bin/perl

# Example 019: Demonstrate external comm utility

use strict;
use warnings;

print "=== Example 019: External comm utility ===\n";

my $a = 'test_comm_a.txt';
my $b = 'test_comm_b.txt';
open my $fa, '>', $a or die "cannot write $a: $!";
open my $fb, '>', $b or die "cannot write $b: $!";
print $fa "apple\nbanana\ncherry\ndate\nelderberry\n";
print $fb "banana\ncherry\nfig\ngrape\nelderberry\n";
close $fa;
close $fb;

# Sort inputs before running comm to avoid warnings and ensure correct behavior
my $as = "$a.sorted";
my $bs = "$b.sorted";
{
    my $cmd = "sort $a > $as";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "sort $b > $bs";
    print "\n$cmd\n";
    print `$cmd`;
}

{
    my $cmd = "comm $as $bs";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "comm -1 $as $bs";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "comm -2 $as $bs";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "comm -3 $as $bs";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "comm -12 $as $bs";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "comm -23 $as $bs";
    print "\n$cmd\n";
    print `$cmd`;
}

unlink $as if -f $as;
unlink $bs if -f $bs;

unlink $a or warn "could not remove $a: $!";
unlink $b or warn "could not remove $b: $!";

print "\n=== Example 019 completed ===\n";
