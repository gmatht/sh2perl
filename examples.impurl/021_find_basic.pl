#!/usr/bin/perl

# Example 021: Demonstrate external find utility

use strict;
use warnings;
use File::Path qw(make_path remove_tree);

print "=== Example 021: External find utility ===\n";

my $root = 'test_find_dir';

sub touch_file {
    my ($path) = @_;
    open my $fh, '>', $path or die "Cannot create $path: $!\n";
    close $fh;
    chmod 0755, $path;
}

remove_tree($root);
make_path("$root/subdir1", "$root/subdir2");
touch_file("$root/file1.txt");
touch_file("$root/file2.pl");
touch_file("$root/subdir1/file3.txt");
touch_file("$root/subdir2/file4.sh");

{
    my $cmd = "find $root -print";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "find $root -name '*.txt' -print";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "find $root -type d -print";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "find $root -maxdepth 1 -print";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "find $root -mindepth 2 -print";
    print "\n$cmd\n";
    print `$cmd`;
}
{
    my $cmd = "find $root -name '*.pl' -exec ls -l {} \\; ";
    print "\n$cmd\n";
    print `$cmd`;
}

remove_tree($root);

print "\n=== Example 021 completed ===\n";
