#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 7 variables: ["echo_result", "printf_result", "sha256_result", "sha512_result", "strings_result", "tee_result", "perl_result"]
my $echo_result = 0;
my $printf_result = 0;
my $sha256_result = 0;
my $sha512_result = 0;
my $strings_result = 0;
my $tee_result = 0;
my $perl_result = 0;

print("=== Output and Formatting Commands ===\n");
$echo_result = `echo "Hello from backticks"`;
$ENV{echo_result} = $echo_result;
print(("Echo result: " . $echo_result) . "\n");
$printf_result = `printf "Number: %d  String: %s\n" 42 "test"`;
$ENV{printf_result} = $printf_result;
print(("Printf result: " . $printf_result) . "\n");
print("=== Compression Commands ===\n");
print("=== Network Commands ===\n");
print("=== Process Management Commands ===\n");
print("=== Checksum Commands ===\n");
local *STDOUT; open(STDOUT, '>', 'test_checksum.txt') or die "Cannot open file: $!\n";
print("test content\n");
$sha256_result = `sha256sum test_checksum.txt`;
$ENV{sha256_result} = $sha256_result;
print(("SHA256 result: " . $sha256_result) . "\n");
$sha512_result = `sha512sum test_checksum.txt`;
$ENV{sha512_result} = $sha512_result;
print(("SHA512 result: " . $sha512_result) . "\n");
$strings_result = `strings test_binary.txt | head -3`;
$ENV{strings_result} = $strings_result;
print("Strings result:\n");
print($strings_result . "\n");
print("=== I/O Redirection Commands ===\n");
$tee_result = `echo "test output" | tee test_tee.txt`;
$ENV{tee_result} = $tee_result;
print(("Tee result: " . $tee_result) . "\n");
print("=== Perl Command ===\n");
$perl_result = `perl -e 'print "Hello from Perl\n"'`;
$ENV{perl_result} = $perl_result;
print(("Perl result: " . $perl_result) . "\n");
unlink('test_checksum.txt');
unlink('test_tee.txt');