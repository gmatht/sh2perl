#!/usr/bin/perl

# Example 016: Demonstrate external sort

use strict;
use warnings;

print "=== Example 016: External sort utility ===\n";

my $file = "test_sort.txt";
open my $fh, '>', $file or die "cannot write $file: $!";
print $fh <<'DATA';
Charlie
Alice
Bob
Diana
Eve
DATA
close $fh;

my $numfile = "test_sort_nums.txt";
open my $nf, '>', $numfile or die "cannot write $numfile: $!";
print $nf <<'NUMS';
25
10
5
30
15
NUMS
close $nf;

{
    my $cmd = "sort $file";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "sort -r $file";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "sort -n $numfile";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "sort -u $file";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "printf '%s\\n' Zebra Apple Banana | sort";
    print "\n$cmd\n";
    print `$cmd`;
}

unlink $file or warn "could not remove $file: $!";
unlink $numfile or warn "could not remove $numfile: $!";

print "\n=== Example 016 completed ===\n";
