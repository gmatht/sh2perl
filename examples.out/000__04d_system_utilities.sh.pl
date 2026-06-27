#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 3 variables: ["formatted_date", "sleep_duration", "yes_result"]
my $formatted_date = 0;
my $sleep_duration = 0;
my $yes_result = 0;

print("=== System Utilities ===\n");
$formatted_date = `date '+%Y-%m-%d'`;
$ENV{formatted_date} = $formatted_date;
print(("Formatted date: " . $formatted_date) . "\n");
$sleep_duration = `echo "1"`;
$ENV{sleep_duration} = $sleep_duration;
print(("Sleeping for " . $sleep_duration . " seconds...") . "\n");
system("sleep", $sleep_duration);
$yes_result = `yes "Hello" | head -3`;
$ENV{yes_result} = $yes_result;
print("Yes command result:\n");
print($yes_result . "\n");
print("=== System Utilities Complete ===\n");