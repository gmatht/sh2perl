#!/usr/bin/env perl
use strict;
use warnings;
use feature 'say';

use File::Path ();
my $d;
my $echo_result;
my $perl_result;
my $printf_result;
my $sha256_result;
my $sha512_result;
my $strings_result;
my $tee_result;

say join(' ', "=== Output and Formatting Commands ===");
$echo_result = do {
my $__cap0 = '';
open(my $__mem0, '>', \$__cap0) or die;
my $__sel0 = select($__mem0);
say join(' ', "Hello from backticks");
close($__mem0);
select($__sel0);
$__cap0 =~ s/\n+$//;
$__cap0
};
say join(' ', "Echo result: " . $echo_result);
$printf_result = do {
my $__cap1 = '';
open(my $__mem1, '>', \$__cap1) or die;
my $__sel1 = select($__mem1);
printf("Number: %d, String: %s\n", "42", "test");
close($__mem1);
select($__sel1);
$__cap1 =~ s/\n+$//;
$__cap1
};
say join(' ', "Printf result: " . $printf_result);
say join(' ', "=== Compression Commands ===");
say join(' ', "=== Network Commands ===");
say join(' ', "=== Process Management Commands ===");
$d = do { my $__qx2 = qx{'mktemp' '-d'}; $__qx2 =~ s/\n+$//; $__qx2 };
(((system("cd", $d)) == 0) || ((system("exit", "1")) == 0));
say join(' ', "=== Checksum Commands ===");
{
        open(my $__sv1, ">&", \*STDOUT) or die;
        open(STDOUT, ">", "test_checksum.txt") or die;
    say join(' ', "test content");
        open(STDOUT, ">&", $__sv1);
}
$sha256_result = do { my $__qx3 = qx{'sha256sum' 'test_checksum.txt'}; $__qx3 =~ s/\n+$//; $__qx3 };
say join(' ', "SHA256 result: " . $sha256_result);
$sha512_result = do { my $__qx4 = qx{'sha512sum' 'test_checksum.txt'}; $__qx4 =~ s/\n+$//; $__qx4 };
say join(' ', "SHA512 result: " . $sha512_result);
# TODO(unsupported): capture body stmt
$strings_result = do { my $__qx5 = qx{}; $__qx5 =~ s/\n+$//; $__qx5 };
say join(' ', "Strings result:");
say join(' ', $strings_result);
say join(' ', "=== I/O Redirection Commands ===");
# TODO(unsupported): capture body stmt
$tee_result = do { my $__qx6 = qx{}; $__qx6 =~ s/\n+$//; $__qx6 };
say join(' ', "Tee result: " . $tee_result);
say join(' ', "=== Perl Command ===");
$perl_result = do { my $__qx7 = qx{'perl' '-e' 'print "Hello from Perl\\\\n"'}; $__qx7 =~ s/\n+$//; $__qx7 };
say join(' ', "Perl result: " . $perl_result);
unlink "\"test_checksum.txt\"", "\"test_tee.txt\"";
chdir("/") or die "cd: $!\n";
File::Path::remove_tree("\$d", undef);
# 2 construct(s) lowered to TODO markers

