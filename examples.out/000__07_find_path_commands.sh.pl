#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 1 variables: ["found_files"]
my $found_files = 0;

$found_files = `find . -name "*.sh" -type f`;
$ENV{found_files} = $found_files;
print("Found shell scripts:\n");
print($found_files . "\n");