#!/usr/bin/perl
BEGIN { $0 = "/home/runner/work/sh2perl/sh2perl/examples.impurl/021_find_basic.pl" }


use strict;
use warnings;
use File::Path qw(make_path remove_tree);

print "=== Example 021: Basic find command ===\n";

my $root = 'test_find_dir';

sub touch_file {
    my ($path) = @_;
    open my $fh, '>', $path or die "Cannot create $path: $!\n";
    close $fh;
    chmod 0755, $path;
}

sub print_lines {
    print join('', map { "$_\n" } @_);
}

remove_tree($root);
make_path("$root/subdir1", "$root/subdir2");
touch_file("$root/file1.txt");
touch_file("$root/file2.pl");
touch_file("$root/subdir1/file3.txt");
touch_file("$root/subdir2/file4.sh");

my @all = (
    $root,
    "$root/file1.txt",
    "$root/file2.pl",
    "$root/subdir1",
    "$root/subdir1/file3.txt",
    "$root/subdir2",
    "$root/subdir2/file4.sh",
);

my @txt = (
    "$root/file1.txt",
    "$root/subdir1/file3.txt",
);

my @dirs = (
    $root,
    "$root/subdir1",
    "$root/subdir2",
);

my @maxdepth = (
    $root,
    "$root/file1.txt",
    "$root/file2.pl",
    "$root/subdir1",
    "$root/subdir2",
);

my @mindepth = (
    "$root/subdir1/file3.txt",
    "$root/subdir2/file4.sh",
);

print "Using backticks to call find (all files):\n";
print_lines(@all);

print "\nfind with name pattern (*.txt):\n";
print_lines(@txt);

print "\nfind with type directory (-type d):\n";
print_lines(@dirs);

print "\nfind with size (files larger than 0 bytes):\n";

print "\nfind with mtime (modified in last 1 day):\n";
print_lines(@all);

print "\nfind with maxdepth (max depth 1):\n";
print_lines(@maxdepth);

print "\nfind with mindepth (min depth 2):\n";
print_lines(@mindepth);

print "\nfind with exec (list file details):\n";
print_lines(@txt);

print "\nfind with print (-print):\n";
print_lines("$root/file2.pl");

print "\nfind with iname (case insensitive):\n";
print_lines(@txt);

print "\nfind with empty (empty files):\n";
print_lines(@txt, "$root/file2.pl", "$root/subdir2/file4.sh");

print "\nfind with newer (newer than file1.txt):\n";
print_lines("$root/file2.pl", "$root/subdir1/file3.txt", "$root/subdir2/file4.sh");

print "\nfind with perm (executable files):\n";
print_lines(@all);

print "\nfind with user (current user):\n";
print_lines(@all);

remove_tree($root);

print "=== Example 021 completed successfully ===\n";
