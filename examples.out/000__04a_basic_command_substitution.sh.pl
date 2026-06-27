#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 2 variables: ["current_date", "current_dir"]
my $current_date = 0;
my $current_dir = 0;

print("=== Basic Command Substitution ===\n");
print("Current date: `date +%Y`\n");
print(("Current directory: `basename " . "$(pwd)`") . "\n");
$current_date = `date +%Y%m`;
$ENV{current_date} = $current_date;
$current_dir = `basename $(pwd)`;
$ENV{current_dir} = $current_dir;
print(("Stored date: " . $current_date) . "\n");
print(("Stored directory: " . $current_dir) . "\n");
print("=== Basic Command Substitution Complete ===\n");