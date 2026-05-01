#!/usr/bin/perl

# Example 017: Demonstrate external uniq

use strict;
use warnings;

print "=== Example 017: External uniq utility ===\n";

my $file = "test_uniq.txt";
open my $fh, '>', $file or die "cannot write $file: $!";
print $fh <<'DATA';
apple
apple
banana
apple
cherry
banana
apple
date
cherry
DATA
close $fh;

{
    my $cmd = "uniq $file";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "uniq -c $file";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "sort $file | uniq -u";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "sort $file | uniq -d";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "sort -f $file | uniq -i";
    print "\n$cmd\n";
    print `$cmd`;
}

unlink $file or warn "could not remove $file: $!";

print "\n=== Example 017 completed ===\n";
