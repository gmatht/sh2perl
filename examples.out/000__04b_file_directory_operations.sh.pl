#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 2 variables: ["file_list", "found_files"]
my $file_list = 0;
my $found_files = 0;

print("=== File and Directory Operations ===\n");
$file_list = `ls -a`;
$ENV{file_list} = $file_list;
print("File listing:\n");
print($file_list . "\n");
$found_files = `find . -name "*.sh" -type f`;
$ENV{found_files} = $found_files;
print("Found shell scripts:\n");
print($found_files . "\n");
print("=== File and Directory Operations Complete ===\n");