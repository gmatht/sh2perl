#!/usr/bin/perl

# Example 029: Demonstrate external xargs utility

use strict;
use warnings;

print "=== Example 029: External xargs utility ===\n";

my $file = 'test_xargs.txt';
open my $fh, '>', $file or die "cannot write $file: $!";
print $fh "file1.txt\nfile2.txt\nfile3.txt\n";
close $fh;

{
    my $cmd = "cat $file | xargs -n 2 echo";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "printf '%s\\0' file1.txt file2.txt | xargs -0 echo";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "cat $file | xargs -I {} echo Processing: {}";
    print "\n$cmd\n";
    print `$cmd`;
}

unlink $file or warn "could not remove $file: $!";

print "\n=== Example 029 completed ===\n";
