#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 3 variables: ["echo_result", "printf_result", "tee_result"]
my $echo_result = 0;
my $printf_result = 0;
my $tee_result = 0;

print("=== Output and Formatting Commands ===\n");
$echo_result = `echo "Hello from backticks"`;
$ENV{echo_result} = $echo_result;
print(("Echo result: " . $echo_result) . "\n");
$printf_result = `printf "Number: %d  String: %s\n" 42 "test"`;
$ENV{printf_result} = $printf_result;
print(("Printf result: " . $printf_result) . "\n");
$tee_result = `echo "test output" | tee test_tee.txt`;
$ENV{tee_result} = $tee_result;
print(("Tee result: " . $tee_result) . "\n");
unlink('test_tee.txt');
print("=== Output and Formatting Commands Complete ===\n");