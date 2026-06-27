#!/usr/bin/env perl
use strict;
use warnings;
use File::Basename;

# DEBUG: Collected 3 variables: ["sha256_result", "sha512_result", "strings_result"]
my $sha256_result = 0;
my $sha512_result = 0;
my $strings_result = 0;

print("=== Checksum Commands ===\n");
local *STDOUT; open(STDOUT, '>', 'test_checksum.txt') or die "Cannot open file: $!\n";
print("test content\n");
$sha256_result = `sha256sum test_checksum.txt`;
$ENV{sha256_result} = $sha256_result;
print(("SHA256 result: " . $sha256_result) . "\n");
$sha512_result = `sha512sum test_checksum.txt`;
$ENV{sha512_result} = $sha512_result;
print(("SHA512 result: " . $sha512_result) . "\n");
$strings_result = `strings target/debug/debashc.exe | head -3`;
$ENV{strings_result} = $strings_result;
print("Strings result:\n");
print($strings_result . "\n");
unlink('test_checksum.txt');
print("=== Checksum Commands Complete ===\n");