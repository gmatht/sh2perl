#!/usr/bin/perl

# Example 015: Demonstrate the external cut utility

use strict;
use warnings;

print "=== Example 015: External cut utility ===\n";

my $file = "test_cut.csv";
open my $fh, '>', $file or die "cannot write $file: $!";
print $fh <<'CSV';
Alice,25,95.5,Engineer
Bob,30,87.2,Manager
Charlie,35,92.8,Developer
Diana,28,88.9,Designer
Eve,32,91.3,Analyst
CSV
close $fh;

{
    my $cmd = "cut -d',' -f1 $file";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "cut -d',' -f1,3 $file";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "cut -d',' -f1-3 $file";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "cut -c1-10 $file";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "echo 'Alice 25 95.5 Engineer' | cut -d' ' -f1,3";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "cut -b1-20 $file";
    print "\n$cmd\n";
    print `$cmd`;
}

unlink $file or warn "could not remove $file: $!";

print "\n=== Example 015 completed ===\n";
