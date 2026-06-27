#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 2 variables: ["path", "s2"]
my $path = 0;
my $s2 = 0;

# set -euo
# set pipefail
print("== Advanced parameter expansion ==\n");
$path = "/tmp/file.txt";
$ENV{path} = $path;
print(basename($path) . "\n");
print(dirname($path) . "\n");
$s2 = "abba";
$ENV{s2} = $s2;
print(do { my $temp = $s2; $temp =~ s/b/X/g; $temp } . "\n");