#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 2 variables: ["formatted_date", "yes_result"]
my $formatted_date = 0;
my $yes_result = 0;

print("=== System Utilities ===\n");
$formatted_date = `date '+%Y-%m-%d'`;
$ENV{formatted_date} = $formatted_date;
print(("Formatted date: " . $formatted_date) . "\n");
$yes_result = `yes "Hello" | head -3`;
$ENV{yes_result} = $yes_result;
print("Yes command result:\n");
print($yes_result . "\n");